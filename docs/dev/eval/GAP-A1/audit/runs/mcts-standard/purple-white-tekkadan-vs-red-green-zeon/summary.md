# purple-white-tekkadan vs red-green-zeon

Deck under test (DUT) **purple-white-tekkadan** against benchmark **red-green-zeon**: 20 BO1 matches, master seed 22710603, AI preset `standard` (agents `mcts-standard` / `mcts-standard`, factory `gcg-ai-reference::make_agent('mcts')`).

Reproduce: `gcg-sim benchmark examples/decks/purple-white-tekkadan.txt examples/decks/red-green-zeon.txt --matches 20 --seed 22710603 --format bo1 --ai-preset standard --replays 20`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 20 | 8 | 12 | 0 | 40.0% [21.9%, 61.3%] (n=20) |
| Games | 20 | 8 | 12 | 0 | 40.0% [21.9%, 61.3%] (n=20) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 9 | 4 | 5 | 0 | 44.4% [18.9%, 73.3%] (n=9) |
| On the draw | 11 | 4 | 7 | 0 | 36.4% [15.2%, 64.6%] (n=11) |
| Game 1 | 20 | 8 | 12 | 0 | 40.0% [21.9%, 61.3%] (n=20) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 20 | 8 | 12 | 0 | 40.0% [21.9%, 61.3%] (n=20) |

Game length (turns): mean 14.1, median 14, p10 10, p90 17, range 10-19. Median by DUT result: win 14, loss 14, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 50.0% [29.9%, 70.1%] (n=20); DUT win rate after a redraw 50.0% [23.7%, 76.3%] (n=10), after keeping 30.0% [10.8%, 60.3%] (n=10). Benchmark redraw rate 30.0% [14.5%, 51.9%] (n=20).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD02-054 Gundam Barbatos 1st Form | 4 | 16 (80.0%) | 16 | 4.2 | 31.2% [14.2%, 55.6%] (n=16) | 75.0% [30.1%, 95.4%] (n=4) | 31.2% [14.2%, 55.6%] (n=16) | 2 (10.0%) | 50.0% |
| GD02-058 Ryusei-Go (Graze Custom Ⅱ) | 2 | 11 (55.0%) | 10 | 5.1 | 54.5% [28.0%, 78.7%] (n=11) | 22.2% [6.3%, 54.7%] (n=9) | 60.0% [31.3%, 83.2%] (n=10) | 1 (9.1%) | 50.0% |
| GD02-114 It's Name is Ryusei-Go | 1 | 4 (20.0%) | 4 | 4.5 | 25.0% [4.6%, 69.9%] (n=4) | 43.8% [23.1%, 66.8%] (n=16) | 25.0% [4.6%, 69.9%] (n=4) | 0 (0.0%) | 80.0% |
| GD03-056 Gundam Barbatos Adapt | 3 | 12 (60.0%) | 12 | 4.2 | 58.3% [32.0%, 80.7%] (n=12) | 12.5% [2.2%, 47.1%] (n=8) | 58.3% [32.0%, 80.7%] (n=12) | 1 (5.9%) | 33.3% |
| GD03-066 Gundam Barbatos 5th Form (Ground Type) | 2 | 13 (65.0%) | 13 | 5.2 | 38.5% [17.7%, 64.5%] (n=13) | 42.9% [15.8%, 75.0%] (n=7) | 38.5% [17.7%, 64.5%] (n=13) | 0 (0.0%) | 40.0% |
| GD05-058 Shiden Custom (Ryusei-Go) | 2 | 9 (45.0%) | 8 | 5.1 | 44.4% [18.9%, 73.3%] (n=9) | 36.4% [15.2%, 64.6%] (n=11) | 37.5% [13.7%, 69.4%] (n=8) | 1 (11.1%) | 66.7% |
| GD05-060 Gundam Flauros (Ryusei-Go) | 2 | 7 (35.0%) | 6 | 5.7 | 57.1% [25.0%, 84.2%] (n=7) | 30.8% [12.7%, 57.6%] (n=13) | 66.7% [30.0%, 90.3%] (n=6) | 3 (30.0%) | n/a |
| GD05-117 Become a Shield | 3 | 14 (70.0%) | 10 | 4.9 | 42.9% [21.4%, 67.4%] (n=14) | 33.3% [9.7%, 70.0%] (n=6) | 40.0% [16.8%, 68.7%] (n=10) | 5 (33.3%) | 80.0% |
| ST01-014 Unforeseen Incident | 3 | 14 (70.0%) | 9 | 6.0 | 50.0% [26.8%, 73.2%] (n=14) | 16.7% [3.0%, 56.4%] (n=6) | 33.3% [12.1%, 64.6%] (n=9) | 6 (33.3%) | 100.0% |
| ST05-001 Gundam Barbatos 4th Form | 3 | 9 (45.0%) | 6 | 6.7 | 33.3% [12.1%, 64.6%] (n=9) | 45.5% [21.3%, 72.0%] (n=11) | 33.3% [9.7%, 70.0%] (n=6) | 6 (50.0%) | 57.1% |
| ST05-002 Gundam Barbatos 2nd Form | 4 | 18 (90.0%) | 16 | 4.3 | 44.4% [24.6%, 66.3%] (n=18) | 0.0% [0.0%, 65.8%] (n=2) | 43.8% [23.1%, 66.8%] (n=16) | 2 (7.1%) | 37.5% |
| ST05-003 CGS Mobile Worker | 4 | 16 (80.0%) | 14 | 3.7 | 43.8% [23.1%, 66.8%] (n=16) | 25.0% [4.6%, 69.9%] (n=4) | 42.9% [21.4%, 67.4%] (n=14) | 10 (34.5%) | 20.0% |
| ST05-004 Graze Custom | 4 | 11 (55.0%) | 9 | 4.8 | 63.6% [35.4%, 84.8%] (n=11) | 11.1% [2.0%, 43.5%] (n=9) | 66.7% [35.4%, 87.9%] (n=9) | 2 (12.5%) | 0.0% |
| ST05-007 McGillis' Schwalbe Graze | 2 | 10 (50.0%) | 8 | 5.0 | 30.0% [10.8%, 60.3%] (n=10) | 50.0% [23.7%, 76.3%] (n=10) | 25.0% [7.1%, 59.1%] (n=8) | 2 (20.0%) | 66.7% |
| ST05-010 Mikazuki Augus | 4 | 18 (90.0%) | 17 | 5.0 | 44.4% [24.6%, 66.3%] (n=18) | 0.0% [0.0%, 65.8%] (n=2) | 47.1% [26.2%, 69.0%] (n=17) | 6 (17.6%) | 33.3% |
| ST05-011 Akihiro Altland | 3 | 16 (80.0%) | 14 | 5.6 | 37.5% [18.5%, 61.4%] (n=16) | 50.0% [15.0%, 85.0%] (n=4) | 42.9% [21.4%, 67.4%] (n=14) | 5 (22.7%) | 75.0% |
| ST05-012 McGillis Fareed | 2 | 11 (55.0%) | 8 | 5.9 | 36.4% [15.2%, 64.6%] (n=11) | 44.4% [18.9%, 73.3%] (n=9) | 25.0% [7.1%, 59.1%] (n=8) | 4 (33.3%) | 50.0% |
| ST05-015 Isaribi | 2 | 13 (65.0%) | 13 | 5.1 | 38.5% [17.7%, 64.5%] (n=13) | 42.9% [15.8%, 75.0%] (n=7) | 38.5% [17.7%, 64.5%] (n=13) | 1 (5.6%) | 0.0% |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.1.0; card data 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42 (built 2026-10-12T14:37:38.154Z); Comprehensive Rules 1.9.0 (2026-09-11); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

## Conflict resolutions affecting these decks

- `ambiguous:breach-reminder` (GD01-030 Rick Dom; benchmark): Apply 13-1-2-1 for every card: <Breach> triggers when, during its controller's turn, the Unit destroys an enemy Unit with battle damage (including battles begun by effects, Q347); reminder wording is ignored.
- `divergent:GD01-030` (GD01-030 Rick Dom; benchmark): Use GD01-030. The printings differ only in parenthesized reminder text.
- `ruling:GD01-059:Q135` (GD01-059 Zee Zulu; benchmark): Conditions and target restrictions of triggered/activated/command effects are evaluated once, when resolved; later changes neither undo the result nor retrigger it.
- `ambiguous:breach-reminder` (GD01-092 M'Quve; benchmark): Apply 13-1-2-1 for every card: <Breach> triggers when, during its controller's turn, the Unit destroys an enemy Unit with battle damage (including battles begun by effects, Q347); reminder wording is ignored.
- `data-null:BASE:ap` (GD01-125 Zanzibar; benchmark): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:GD01-125:Q157` (GD01-125 Zanzibar; benchmark): A card deployed, paired or activated by an effect pays no cost and needs no Lv. check unless the text says 'Pay its cost'; its own 【Deploy】/【When Paired】/【When Linked】 still trigger.
- `ruling:GD03-056:Q230` (GD03-056 Gundam Barbatos Adapt; deck_under_test): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `ruling:GD05-060:Q372` (GD05-060 Gundam Flauros (Ryusei-Go); deck_under_test): '【A】/【B】' gives the effect both timings; each triggers independently (no shared once-per-turn).
- `ruling:GD05-060:Q373` (GD05-060 Gundam Flauros (Ryusei-Go); deck_under_test): '【A】/【B】' gives the effect both timings; each triggers independently (no shared once-per-turn).
- `ruling:GD05-117:Q412` (GD05-117 Become a Shield; deck_under_test): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `divergent:R-001` (R-001 Resource; benchmark, deck_under_test): Use R-001_p7: a Resource card has no AP or HP. The Edition Beta record R-001_p4 (AP3/HP1) is a data error.
- `divergent:ST03-001` (ST03-001 Sinanju; benchmark): Use ST03-001. The SC01 reprint (_p2) lists the two effects in the other order, which changes nothing: they are separate effects (ruling Q164).
- `ruling:ST03-001:Q164` (ST03-001 Sinanju; benchmark): An effect gated by 【During Pair】/【During Link】/'while …' is not possessed while the gate is closed (so it does not satisfy 'Unit with a 【Destroyed】 effect' or 'Unit card with <Keyword>'); each 【During Pair】 prefix governs only its own effect.
- `ruling:ST03-010:Q145` (ST03-010 Full Frontal; benchmark): A card deployed, paired or activated by an effect pays no cost and needs no Lv. check unless the text says 'Pay its cost'; its own 【Deploy】/【When Paired】/【When Linked】 still trigger.
- `ambiguous:suppression-with-base` (ST05-001 Gundam Barbatos 4th Form; deck_under_test): With a Base in the shield area the attack damages only the Base (<Suppression> does nothing, excess is lost). Without a Base, <Suppression> deals the attacker's battle damage to the top two Shields simultaneously (one if only one remains, 13-1-7-3).
- `divergent:ST05-001` (ST05-001 Gundam Barbatos 4th Form; deck_under_test): Use ST05-001. The SC01 reprint (_p3) lists the effects in the other order, which changes nothing.
- `data-null:UNIT:ap` (ST05-003 CGS Mobile Worker; deck_under_test): These Units have no AP value; treat AP as 0 (they deal no battle damage, 5-5-5).
- `ruling:ST05-010:Q168` (ST05-010 Mikazuki Augus; deck_under_test): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `ruling:ST05-011:Q169` (ST05-011 Akihiro Altland; deck_under_test): 'When this Unit destroys an enemy … with (battle) damage' triggers even if this Unit is destroyed simultaneously; the pending trigger resolves after both Units are in the trash (parts that need this Unit on the field do nothing).
- `data-null:BASE:ap` (ST05-015 Isaribi; deck_under_test): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:ST05-015:Q170` (ST05-015 Isaribi; deck_under_test): Conditions and target restrictions of triggered/activated/command effects are evaluated once, when resolved; later changes neither undo the result nor retrigger it.

## Replays

- `replays/win-m0001-g1.json`: win, match 1 game 1, 14 turns, battle_damage
- `replays/win-m0007-g1.json`: win, match 7 game 1, 13 turns, battle_damage
- `replays/win-m0008-g1.json`: win, match 8 game 1, 16 turns, battle_damage
- `replays/win-m0009-g1.json`: win, match 9 game 1, 15 turns, battle_damage
- `replays/win-m0010-g1.json`: win, match 10 game 1, 12 turns, battle_damage
- `replays/win-m0011-g1.json`: win, match 11 game 1, 14 turns, battle_damage
- `replays/win-m0016-g1.json`: win, match 16 game 1, 13 turns, battle_damage
- `replays/win-m0017-g1.json`: win, match 17 game 1, 17 turns, battle_damage
- `replays/loss-m0000-g1.json`: loss, match 0 game 1, 14 turns, battle_damage
- `replays/loss-m0002-g1.json`: loss, match 2 game 1, 10 turns, battle_damage
- `replays/loss-m0003-g1.json`: loss, match 3 game 1, 14 turns, battle_damage
- `replays/loss-m0004-g1.json`: loss, match 4 game 1, 15 turns, battle_damage
- `replays/loss-m0005-g1.json`: loss, match 5 game 1, 13 turns, battle_damage
- `replays/loss-m0006-g1.json`: loss, match 6 game 1, 14 turns, battle_damage
- `replays/loss-m0012-g1.json`: loss, match 12 game 1, 10 turns, battle_damage
- `replays/loss-m0013-g1.json`: loss, match 13 game 1, 19 turns, battle_damage
- `replays/loss-m0014-g1.json`: loss, match 14 game 1, 19 turns, battle_damage
- `replays/loss-m0015-g1.json`: loss, match 15 game 1, 11 turns, battle_damage
- `replays/loss-m0018-g1.json`: loss, match 18 game 1, 13 turns, battle_damage
- `replays/loss-m0019-g1.json`: loss, match 19 game 1, 15 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
