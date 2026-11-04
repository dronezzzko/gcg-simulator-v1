# purple-white-tekkadan vs red-green-zeon

Deck under test (DUT) **purple-white-tekkadan** against benchmark **red-green-zeon**: 20 BO1 matches, master seed 22710603, AI preset `standard` (agents `search-standard` / `search-standard`, factory `gcg-search::make_agent('standard')`).

Search: `gcg-search`, preset `standard` (reference strong mapping), 1024 iterations per option (min 2560, max 10240, auxiliary apply cap 16 per iteration), mechanisms: `adaptive_horizon`, `capped_rollout_continuation`, `engine_error_leaf`, `exclude_capped_play_draw`, `kill_aware_targeting`, `tactical_solver`, `tree_lethal_select`, weights `ROLE_WEIGHTS`.

Reproduce: `gcg-sim benchmark examples/decks/purple-white-tekkadan.txt examples/decks/red-green-zeon.txt --matches 20 --seed 22710603 --format bo1 --ai-preset standard --replays 20`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 20 | 3 | 17 | 0 | 15.0% [5.2%, 36.0%] (n=20) |
| Games | 20 | 3 | 17 | 0 | 15.0% [5.2%, 36.0%] (n=20) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 9 | 1 | 8 | 0 | 11.1% [2.0%, 43.5%] (n=9) |
| On the draw | 11 | 2 | 9 | 0 | 18.2% [5.1%, 47.7%] (n=11) |
| Game 1 | 20 | 3 | 17 | 0 | 15.0% [5.2%, 36.0%] (n=20) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 20 | 3 | 17 | 0 | 15.0% [5.2%, 36.0%] (n=20) |

Game length (turns): mean 14.5, median 14, p10 12, p90 18, range 10-18. Median by DUT result: win 16, loss 14, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 50.0% [29.9%, 70.1%] (n=20); DUT win rate after a redraw 0.0% [0.0%, 27.8%] (n=10), after keeping 30.0% [10.8%, 60.3%] (n=10). Benchmark redraw rate 25.0% [11.2%, 46.9%] (n=20).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD02-054 Gundam Barbatos 1st Form | 4 | 18 (90.0%) | 18 | 4.0 | 16.7% [5.8%, 39.2%] (n=18) | 0.0% [0.0%, 65.8%] (n=2) | 16.7% [5.8%, 39.2%] (n=18) | 1 (4.5%) | 50.0% |
| GD02-058 Ryusei-Go (Graze Custom Ⅱ) | 2 | 10 (50.0%) | 8 | 5.5 | 20.0% [5.7%, 51.0%] (n=10) | 10.0% [1.8%, 40.4%] (n=10) | 25.0% [7.1%, 59.1%] (n=8) | 2 (20.0%) | 50.0% |
| GD02-114 It's Name is Ryusei-Go | 1 | 4 (20.0%) | 3 | 4.3 | 0.0% [0.0%, 49.0%] (n=4) | 18.8% [6.6%, 43.0%] (n=16) | 0.0% [0.0%, 56.2%] (n=3) | 1 (25.0%) | 80.0% |
| GD03-056 Gundam Barbatos Adapt | 3 | 11 (55.0%) | 11 | 4.3 | 27.3% [9.7%, 56.6%] (n=11) | 0.0% [0.0%, 29.9%] (n=9) | 27.3% [9.7%, 56.6%] (n=11) | 3 (18.8%) | 33.3% |
| GD03-066 Gundam Barbatos 5th Form (Ground Type) | 2 | 14 (70.0%) | 13 | 5.3 | 7.1% [1.3%, 31.5%] (n=14) | 33.3% [9.7%, 70.0%] (n=6) | 7.7% [1.4%, 33.3%] (n=13) | 2 (13.3%) | 40.0% |
| GD05-058 Shiden Custom (Ryusei-Go) | 2 | 9 (45.0%) | 9 | 5.3 | 33.3% [12.1%, 64.6%] (n=9) | 0.0% [0.0%, 25.9%] (n=11) | 33.3% [12.1%, 64.6%] (n=9) | 0 (0.0%) | 66.7% |
| GD05-060 Gundam Flauros (Ryusei-Go) | 2 | 8 (40.0%) | 7 | 5.7 | 12.5% [2.2%, 47.1%] (n=8) | 16.7% [4.7%, 44.8%] (n=12) | 14.3% [2.6%, 51.3%] (n=7) | 3 (30.0%) | n/a |
| GD05-117 Become a Shield | 3 | 14 (70.0%) | 13 | 3.8 | 14.3% [4.0%, 39.9%] (n=14) | 16.7% [3.0%, 56.4%] (n=6) | 15.4% [4.3%, 42.2%] (n=13) | 1 (7.1%) | 80.0% |
| ST01-014 Unforeseen Incident | 3 | 13 (65.0%) | 12 | 5.2 | 15.4% [4.3%, 42.2%] (n=13) | 14.3% [2.6%, 51.3%] (n=7) | 8.3% [1.5%, 35.4%] (n=12) | 3 (16.7%) | 100.0% |
| ST05-001 Gundam Barbatos 4th Form | 3 | 10 (50.0%) | 10 | 6.5 | 20.0% [5.7%, 51.0%] (n=10) | 10.0% [1.8%, 40.4%] (n=10) | 20.0% [5.7%, 51.0%] (n=10) | 0 (0.0%) | 57.1% |
| ST05-002 Gundam Barbatos 2nd Form | 4 | 17 (85.0%) | 17 | 4.4 | 17.6% [6.2%, 41.0%] (n=17) | 0.0% [0.0%, 56.2%] (n=3) | 17.6% [6.2%, 41.0%] (n=17) | 1 (3.8%) | 37.5% |
| ST05-003 CGS Mobile Worker | 4 | 17 (85.0%) | 16 | 3.0 | 11.8% [3.3%, 34.3%] (n=17) | 33.3% [6.1%, 79.2%] (n=3) | 12.5% [3.5%, 36.0%] (n=16) | 5 (17.9%) | 20.0% |
| ST05-004 Graze Custom | 4 | 9 (45.0%) | 8 | 4.9 | 11.1% [2.0%, 43.5%] (n=9) | 18.2% [5.1%, 47.7%] (n=11) | 12.5% [2.2%, 47.1%] (n=8) | 1 (8.3%) | 0.0% |
| ST05-007 McGillis' Schwalbe Graze | 2 | 13 (65.0%) | 11 | 5.6 | 23.1% [8.2%, 50.3%] (n=13) | 0.0% [0.0%, 35.4%] (n=7) | 27.3% [9.7%, 56.6%] (n=11) | 2 (15.4%) | 66.7% |
| ST05-010 Mikazuki Augus | 4 | 17 (85.0%) | 17 | 4.3 | 17.6% [6.2%, 41.0%] (n=17) | 0.0% [0.0%, 56.2%] (n=3) | 17.6% [6.2%, 41.0%] (n=17) | 2 (6.5%) | 33.3% |
| ST05-011 Akihiro Altland | 3 | 17 (85.0%) | 15 | 5.8 | 17.6% [6.2%, 41.0%] (n=17) | 0.0% [0.0%, 56.2%] (n=3) | 20.0% [7.0%, 45.2%] (n=15) | 7 (26.9%) | 75.0% |
| ST05-012 McGillis Fareed | 2 | 9 (45.0%) | 8 | 6.4 | 11.1% [2.0%, 43.5%] (n=9) | 18.2% [5.1%, 47.7%] (n=11) | 12.5% [2.2%, 47.1%] (n=8) | 2 (20.0%) | 50.0% |
| ST05-015 Isaribi | 2 | 13 (65.0%) | 13 | 4.8 | 7.7% [1.4%, 33.3%] (n=13) | 28.6% [8.2%, 64.1%] (n=7) | 7.7% [1.4%, 33.3%] (n=13) | 2 (10.0%) | 0.0% |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.2.0; card data 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42 (built 2026-10-12T14:37:38.154Z); Comprehensive Rules 1.9.0 (2026-09-11); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

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

- `replays/win-m0007-g1.json`: win, match 7 game 1, 13 turns, battle_damage
- `replays/win-m0008-g1.json`: win, match 8 game 1, 18 turns, battle_damage
- `replays/win-m0015-g1.json`: win, match 15 game 1, 16 turns, battle_damage
- `replays/loss-m0000-g1.json`: loss, match 0 game 1, 12 turns, battle_damage
- `replays/loss-m0001-g1.json`: loss, match 1 game 1, 13 turns, battle_damage
- `replays/loss-m0002-g1.json`: loss, match 2 game 1, 10 turns, battle_damage
- `replays/loss-m0003-g1.json`: loss, match 3 game 1, 14 turns, battle_damage
- `replays/loss-m0004-g1.json`: loss, match 4 game 1, 15 turns, battle_damage
- `replays/loss-m0005-g1.json`: loss, match 5 game 1, 13 turns, battle_damage
- `replays/loss-m0006-g1.json`: loss, match 6 game 1, 14 turns, battle_damage
- `replays/loss-m0009-g1.json`: loss, match 9 game 1, 16 turns, battle_damage
- `replays/loss-m0010-g1.json`: loss, match 10 game 1, 17 turns, battle_damage
- `replays/loss-m0011-g1.json`: loss, match 11 game 1, 13 turns, battle_damage
- `replays/loss-m0012-g1.json`: loss, match 12 game 1, 12 turns, battle_damage
- `replays/loss-m0013-g1.json`: loss, match 13 game 1, 15 turns, battle_damage
- `replays/loss-m0014-g1.json`: loss, match 14 game 1, 13 turns, battle_damage
- `replays/loss-m0016-g1.json`: loss, match 16 game 1, 18 turns, battle_damage
- `replays/loss-m0017-g1.json`: loss, match 17 game 1, 18 turns, battle_damage
- `replays/loss-m0018-g1.json`: loss, match 18 game 1, 13 turns, battle_damage
- `replays/loss-m0019-g1.json`: loss, match 19 game 1, 17 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
