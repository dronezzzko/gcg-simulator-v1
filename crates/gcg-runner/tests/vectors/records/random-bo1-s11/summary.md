# red-green-zeon vs purple-white-tekkadan

Deck under test (DUT) **red-green-zeon** against benchmark **purple-white-tekkadan**: 40 BO1 matches, master seed 11, AI preset `strong` (agents `test-random` / `test-random`, factory `random_agent.py:make_random_agent`).

Reproduce: `gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/purple-white-tekkadan.txt --matches 40 --seed 11 --format bo1 --ai-preset strong --replays 3`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 40 | 28 | 12 | 0 | 70.0% [54.6%, 81.9%] (n=40) |
| Games | 40 | 28 | 12 | 0 | 70.0% [54.6%, 81.9%] (n=40) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 21 | 15 | 6 | 0 | 71.4% [50.0%, 86.2%] (n=21) |
| On the draw | 19 | 13 | 6 | 0 | 68.4% [46.0%, 84.6%] (n=19) |
| Game 1 | 40 | 28 | 12 | 0 | 70.0% [54.6%, 81.9%] (n=40) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 40 | 28 | 12 | 0 | 70.0% [54.6%, 81.9%] (n=40) |

Game length (turns): mean 18.5, median 19, p10 13, p90 24, range 10-29. Median by DUT result: win 19, loss 18, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 45.0% [30.7%, 60.2%] (n=40); DUT win rate after a redraw 55.6% [33.7%, 75.4%] (n=18), after keeping 81.8% [61.5%, 92.7%] (n=22). Benchmark redraw rate 50.0% [35.2%, 64.8%] (n=40).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-030 Rick Dom | 4 | 30 (75.0%) | 27 | 6.0 | 80.0% [62.7%, 90.5%] (n=30) | 40.0% [16.8%, 68.7%] (n=10) | 77.8% [59.2%, 89.4%] (n=27) | 9 (17.0%) | 35.7% |
| GD01-031 Gelgoog | 4 | 28 (70.0%) | 26 | 6.4 | 78.6% [60.5%, 89.8%] (n=28) | 50.0% [25.4%, 74.6%] (n=12) | 84.6% [66.5%, 93.9%] (n=26) | 11 (23.4%) | 50.0% |
| GD01-035 Zaku Ⅱ | 3 | 22 (55.0%) | 20 | 4.5 | 68.2% [47.3%, 83.6%] (n=22) | 72.2% [49.1%, 87.5%] (n=18) | 75.0% [53.1%, 88.8%] (n=20) | 4 (13.3%) | 50.0% |
| GD01-044 Kshatriya | 2 | 20 (50.0%) | 18 | 7.6 | 75.0% [53.1%, 88.8%] (n=20) | 65.0% [43.3%, 81.9%] (n=20) | 77.8% [54.8%, 91.0%] (n=18) | 4 (16.7%) | 50.0% |
| GD01-059 Zee Zulu | 4 | 33 (82.5%) | 29 | 5.8 | 75.8% [59.0%, 87.2%] (n=33) | 42.9% [15.8%, 75.0%] (n=7) | 75.9% [57.9%, 87.8%] (n=29) | 8 (16.0%) | 35.7% |
| GD01-092 M'Quve | 3 | 32 (80.0%) | 28 | 5.6 | 65.6% [48.3%, 79.6%] (n=32) | 87.5% [52.9%, 97.8%] (n=8) | 71.4% [52.9%, 84.7%] (n=28) | 5 (12.2%) | 46.7% |
| GD01-093 Marida Cruz | 2 | 21 (52.5%) | 20 | 5.8 | 66.7% [45.4%, 82.8%] (n=21) | 73.7% [51.2%, 88.2%] (n=19) | 70.0% [48.1%, 85.5%] (n=20) | 1 (4.0%) | 36.4% |
| GD01-115 Zeon Remnant Forces | 3 | 26 (65.0%) | 26 | 4.4 | 65.4% [46.2%, 80.6%] (n=26) | 78.6% [52.4%, 92.4%] (n=14) | 65.4% [46.2%, 80.6%] (n=26) | 0 (0.0%) | 30.0% |
| GD01-125 Zanzibar | 2 | 16 (40.0%) | 15 | 7.0 | 75.0% [50.5%, 89.8%] (n=16) | 66.7% [46.7%, 82.0%] (n=24) | 73.3% [48.0%, 89.1%] (n=15) | 2 (11.1%) | 75.0% |
| GD02-048 Zaku Ⅲ (Sleeves) | 3 | 25 (62.5%) | 23 | 5.8 | 60.0% [40.7%, 76.6%] (n=25) | 86.7% [62.1%, 96.3%] (n=15) | 65.2% [44.9%, 81.2%] (n=23) | 4 (11.4%) | 50.0% |
| GD04-040 Schuzrum-Galluss | 2 | 25 (62.5%) | 22 | 6.9 | 56.0% [37.1%, 73.3%] (n=25) | 93.3% [70.2%, 98.8%] (n=15) | 59.1% [38.7%, 76.7%] (n=22) | 6 (19.4%) | 40.0% |
| ST03-001 Sinanju | 2 | 20 (50.0%) | 14 | 7.9 | 75.0% [53.1%, 88.8%] (n=20) | 65.0% [43.3%, 81.9%] (n=20) | 85.7% [60.1%, 96.0%] (n=14) | 9 (39.1%) | 62.5% |
| ST03-003 Geara Zulu | 2 | 19 (47.5%) | 17 | 5.7 | 63.2% [41.0%, 80.9%] (n=19) | 76.2% [54.9%, 89.4%] (n=21) | 64.7% [41.3%, 82.7%] (n=17) | 3 (14.3%) | 75.0% |
| ST03-006 Char's Zaku Ⅱ | 3 | 29 (72.5%) | 27 | 6.2 | 69.0% [50.8%, 82.7%] (n=29) | 72.7% [43.4%, 90.3%] (n=11) | 70.4% [51.5%, 84.1%] (n=27) | 3 (7.5%) | 45.5% |
| ST03-008 Zaku Ⅱ | 4 | 31 (77.5%) | 28 | 4.9 | 67.7% [50.1%, 81.4%] (n=31) | 77.8% [45.3%, 93.7%] (n=9) | 71.4% [52.9%, 84.7%] (n=28) | 11 (20.0%) | 44.4% |
| ST03-010 Full Frontal | 2 | 16 (40.0%) | 13 | 7.4 | 75.0% [50.5%, 89.8%] (n=16) | 66.7% [46.7%, 82.0%] (n=24) | 76.9% [49.7%, 91.8%] (n=13) | 3 (16.7%) | 22.2% |
| ST03-012 Indignation | 2 | 22 (55.0%) | 22 | 4.9 | 63.6% [43.0%, 80.3%] (n=22) | 77.8% [54.8%, 91.0%] (n=18) | 63.6% [43.0%, 80.3%] (n=22) | 0 (0.0%) | 22.2% |
| ST03-013 Close Combat | 3 | 31 (77.5%) | 30 | 5.4 | 67.7% [50.1%, 81.4%] (n=31) | 77.8% [45.3%, 93.7%] (n=9) | 66.7% [48.8%, 80.8%] (n=30) | 1 (2.8%) | 54.5% |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.1.0; card data 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42 (built 2026-10-12T14:37:38.154Z); Comprehensive Rules 1.9.0 (2026-09-11); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

## Conflict resolutions affecting these decks

- `ambiguous:breach-reminder` (GD01-030 Rick Dom; deck_under_test): Apply 13-1-2-1 for every card: <Breach> triggers when, during its controller's turn, the Unit destroys an enemy Unit with battle damage (including battles begun by effects, Q347); reminder wording is ignored.
- `divergent:GD01-030` (GD01-030 Rick Dom; deck_under_test): Use GD01-030. The printings differ only in parenthesized reminder text.
- `ruling:GD01-059:Q135` (GD01-059 Zee Zulu; deck_under_test): Conditions and target restrictions of triggered/activated/command effects are evaluated once, when resolved; later changes neither undo the result nor retrigger it.
- `ambiguous:breach-reminder` (GD01-092 M'Quve; deck_under_test): Apply 13-1-2-1 for every card: <Breach> triggers when, during its controller's turn, the Unit destroys an enemy Unit with battle damage (including battles begun by effects, Q347); reminder wording is ignored.
- `data-null:BASE:ap` (GD01-125 Zanzibar; deck_under_test): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:GD01-125:Q157` (GD01-125 Zanzibar; deck_under_test): A card deployed, paired or activated by an effect pays no cost and needs no Lv. check unless the text says 'Pay its cost'; its own 【Deploy】/【When Paired】/【When Linked】 still trigger.
- `ruling:GD03-056:Q230` (GD03-056 Gundam Barbatos Adapt; benchmark): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `ruling:GD05-060:Q372` (GD05-060 Gundam Flauros (Ryusei-Go); benchmark): '【A】/【B】' gives the effect both timings; each triggers independently (no shared once-per-turn).
- `ruling:GD05-060:Q373` (GD05-060 Gundam Flauros (Ryusei-Go); benchmark): '【A】/【B】' gives the effect both timings; each triggers independently (no shared once-per-turn).
- `ruling:GD05-117:Q412` (GD05-117 Become a Shield; benchmark): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `divergent:R-001` (R-001 Resource; benchmark, deck_under_test): Use R-001_p7: a Resource card has no AP or HP. The Edition Beta record R-001_p4 (AP3/HP1) is a data error.
- `divergent:ST03-001` (ST03-001 Sinanju; deck_under_test): Use ST03-001. The SC01 reprint (_p2) lists the two effects in the other order, which changes nothing: they are separate effects (ruling Q164).
- `ruling:ST03-001:Q164` (ST03-001 Sinanju; deck_under_test): An effect gated by 【During Pair】/【During Link】/'while …' is not possessed while the gate is closed (so it does not satisfy 'Unit with a 【Destroyed】 effect' or 'Unit card with <Keyword>'); each 【During Pair】 prefix governs only its own effect.
- `ruling:ST03-010:Q145` (ST03-010 Full Frontal; deck_under_test): A card deployed, paired or activated by an effect pays no cost and needs no Lv. check unless the text says 'Pay its cost'; its own 【Deploy】/【When Paired】/【When Linked】 still trigger.
- `ambiguous:suppression-with-base` (ST05-001 Gundam Barbatos 4th Form; benchmark): With a Base in the shield area the attack damages only the Base (<Suppression> does nothing, excess is lost). Without a Base, <Suppression> deals the attacker's battle damage to the top two Shields simultaneously (one if only one remains, 13-1-7-3).
- `divergent:ST05-001` (ST05-001 Gundam Barbatos 4th Form; benchmark): Use ST05-001. The SC01 reprint (_p3) lists the effects in the other order, which changes nothing.
- `data-null:UNIT:ap` (ST05-003 CGS Mobile Worker; benchmark): These Units have no AP value; treat AP as 0 (they deal no battle damage, 5-5-5).
- `ruling:ST05-010:Q168` (ST05-010 Mikazuki Augus; benchmark): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `ruling:ST05-011:Q169` (ST05-011 Akihiro Altland; benchmark): 'When this Unit destroys an enemy … with (battle) damage' triggers even if this Unit is destroyed simultaneously; the pending trigger resolves after both Units are in the trash (parts that need this Unit on the field do nothing).
- `data-null:BASE:ap` (ST05-015 Isaribi; benchmark): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:ST05-015:Q170` (ST05-015 Isaribi; benchmark): Conditions and target restrictions of triggered/activated/command effects are evaluated once, when resolved; later changes neither undo the result nor retrigger it.

## Replays

- `replays/win-m0009-g1.json`: win, match 9 game 1, 19 turns, battle_damage
- `replays/win-m0018-g1.json`: win, match 18 game 1, 19 turns, battle_damage
- `replays/win-m0035-g1.json`: win, match 35 game 1, 19 turns, battle_damage
- `replays/loss-m0010-g1.json`: loss, match 10 game 1, 17 turns, battle_damage
- `replays/loss-m0021-g1.json`: loss, match 21 game 1, 19 turns, battle_damage
- `replays/loss-m0033-g1.json`: loss, match 33 game 1, 18 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
