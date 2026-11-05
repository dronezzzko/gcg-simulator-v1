# red-green-zeon vs blue-white-federation

Deck under test (DUT) **red-green-zeon** against benchmark **blue-white-federation**: 20 BO1 matches, master seed 22710601, AI preset `standard` (agents `search-standard` / `search-standard`, factory `gcg-search::make_agent('standard')`).

Search: `gcg-search`, preset `standard` (reference strong mapping), 1024 iterations per option (min 2560, max 10240, auxiliary apply cap 16 per iteration), mechanisms: `adaptive_horizon`, `capped_rollout_continuation`, `engine_error_leaf`, `exclude_capped_play_draw`, `kill_aware_targeting`, `tactical_solver`, `tree_lethal_select`, weights `ROLE_WEIGHTS`.

Reproduce: `gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt --matches 20 --seed 22710601 --format bo1 --ai-preset standard --replays 20`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 20 | 9 | 11 | 0 | 45.0% [25.8%, 65.8%] (n=20) |
| Games | 20 | 9 | 11 | 0 | 45.0% [25.8%, 65.8%] (n=20) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 15 | 7 | 8 | 0 | 46.7% [24.8%, 69.9%] (n=15) |
| On the draw | 5 | 2 | 3 | 0 | 40.0% [11.8%, 76.9%] (n=5) |
| Game 1 | 20 | 9 | 11 | 0 | 45.0% [25.8%, 65.8%] (n=20) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 20 | 9 | 11 | 0 | 45.0% [25.8%, 65.8%] (n=20) |

Game length (turns): mean 17.0, median 17, p10 11, p90 20, range 11-22. Median by DUT result: win 16, loss 18, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 20.0% [8.1%, 41.6%] (n=20); DUT win rate after a redraw 50.0% [15.0%, 85.0%] (n=4), after keeping 43.8% [23.1%, 66.8%] (n=16). Benchmark redraw rate 50.0% [29.9%, 70.1%] (n=20).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-030 Rick Dom | 4 | 15 (75.0%) | 14 | 4.4 | 53.3% [30.1%, 75.2%] (n=15) | 20.0% [3.6%, 62.4%] (n=5) | 50.0% [26.8%, 73.2%] (n=14) | 1 (4.3%) | 14.3% |
| GD01-031 Gelgoog | 4 | 16 (80.0%) | 15 | 5.7 | 50.0% [28.0%, 72.0%] (n=16) | 25.0% [4.6%, 69.9%] (n=4) | 46.7% [24.8%, 69.9%] (n=15) | 5 (17.9%) | 33.3% |
| GD01-035 Zaku Ⅱ | 3 | 15 (75.0%) | 15 | 4.5 | 40.0% [19.8%, 64.3%] (n=15) | 60.0% [23.1%, 88.2%] (n=5) | 40.0% [19.8%, 64.3%] (n=15) | 0 (0.0%) | 0.0% |
| GD01-044 Kshatriya | 2 | 11 (55.0%) | 11 | 5.7 | 54.5% [28.0%, 78.7%] (n=11) | 33.3% [12.1%, 64.6%] (n=9) | 54.5% [28.0%, 78.7%] (n=11) | 0 (0.0%) | 0.0% |
| GD01-059 Zee Zulu | 4 | 16 (80.0%) | 16 | 5.4 | 43.8% [23.1%, 66.8%] (n=16) | 50.0% [15.0%, 85.0%] (n=4) | 43.8% [23.1%, 66.8%] (n=16) | 2 (6.1%) | 10.0% |
| GD01-092 M'Quve | 3 | 12 (60.0%) | 11 | 5.8 | 41.7% [19.3%, 68.0%] (n=12) | 50.0% [21.5%, 78.5%] (n=8) | 36.4% [15.2%, 64.6%] (n=11) | 5 (26.3%) | 0.0% |
| GD01-093 Marida Cruz | 2 | 16 (80.0%) | 15 | 5.3 | 50.0% [28.0%, 72.0%] (n=16) | 25.0% [4.6%, 69.9%] (n=4) | 53.3% [30.1%, 75.2%] (n=15) | 2 (9.5%) | 16.7% |
| GD01-115 Zeon Remnant Forces | 3 | 13 (65.0%) | 12 | 6.3 | 46.2% [23.2%, 70.9%] (n=13) | 42.9% [15.8%, 75.0%] (n=7) | 41.7% [19.3%, 68.0%] (n=12) | 1 (6.7%) | 16.7% |
| GD01-125 Zanzibar | 2 | 8 (40.0%) | 8 | 5.0 | 37.5% [13.7%, 69.4%] (n=8) | 50.0% [25.4%, 74.6%] (n=12) | 37.5% [13.7%, 69.4%] (n=8) | 0 (0.0%) | 66.7% |
| GD02-048 Zaku Ⅲ (Sleeves) | 3 | 9 (45.0%) | 9 | 5.9 | 33.3% [12.1%, 64.6%] (n=9) | 54.5% [28.0%, 78.7%] (n=11) | 33.3% [12.1%, 64.6%] (n=9) | 0 (0.0%) | 16.7% |
| GD04-040 Schuzrum-Galluss | 2 | 12 (60.0%) | 11 | 4.8 | 50.0% [25.4%, 74.6%] (n=12) | 37.5% [13.7%, 69.4%] (n=8) | 45.5% [21.3%, 72.0%] (n=11) | 1 (7.7%) | 20.0% |
| ST03-001 Sinanju | 2 | 10 (50.0%) | 10 | 6.8 | 60.0% [31.3%, 83.2%] (n=10) | 30.0% [10.8%, 60.3%] (n=10) | 60.0% [31.3%, 83.2%] (n=10) | 1 (9.1%) | 66.7% |
| ST03-003 Geara Zulu | 2 | 12 (60.0%) | 10 | 5.8 | 58.3% [32.0%, 80.7%] (n=12) | 25.0% [7.1%, 59.1%] (n=8) | 60.0% [31.3%, 83.2%] (n=10) | 2 (13.3%) | 0.0% |
| ST03-006 Char's Zaku Ⅱ | 3 | 14 (70.0%) | 13 | 4.7 | 50.0% [26.8%, 73.2%] (n=14) | 33.3% [9.7%, 70.0%] (n=6) | 53.8% [29.1%, 76.8%] (n=13) | 5 (23.8%) | 33.3% |
| ST03-008 Zaku Ⅱ | 4 | 17 (85.0%) | 16 | 3.4 | 47.1% [26.2%, 69.0%] (n=17) | 33.3% [6.1%, 79.2%] (n=3) | 43.8% [23.1%, 66.8%] (n=16) | 1 (4.8%) | 0.0% |
| ST03-010 Full Frontal | 2 | 12 (60.0%) | 11 | 6.5 | 33.3% [13.8%, 60.9%] (n=12) | 62.5% [30.6%, 86.3%] (n=8) | 27.3% [9.7%, 56.6%] (n=11) | 2 (15.4%) | 60.0% |
| ST03-012 Indignation | 2 | 12 (60.0%) | 12 | 6.1 | 41.7% [19.3%, 68.0%] (n=12) | 50.0% [21.5%, 78.5%] (n=8) | 41.7% [19.3%, 68.0%] (n=12) | 0 (0.0%) | 0.0% |
| ST03-013 Close Combat | 3 | 8 (40.0%) | 8 | 5.8 | 50.0% [21.5%, 78.5%] (n=8) | 41.7% [19.3%, 68.0%] (n=12) | 50.0% [21.5%, 78.5%] (n=8) | 0 (0.0%) | 66.7% |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.2.0; card data 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42 (built 2026-10-12T14:37:38.154Z); Comprehensive Rules 1.9.0 (2026-09-11); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

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

- `replays/win-m0003-g1.json`: win, match 3 game 1, 11 turns, battle_damage
- `replays/win-m0008-g1.json`: win, match 8 game 1, 15 turns, battle_damage
- `replays/win-m0009-g1.json`: win, match 9 game 1, 20 turns, battle_damage
- `replays/win-m0010-g1.json`: win, match 10 game 1, 19 turns, battle_damage
- `replays/win-m0011-g1.json`: win, match 11 game 1, 11 turns, battle_damage
- `replays/win-m0012-g1.json`: win, match 12 game 1, 16 turns, battle_damage
- `replays/win-m0013-g1.json`: win, match 13 game 1, 21 turns, battle_damage
- `replays/win-m0017-g1.json`: win, match 17 game 1, 11 turns, battle_damage
- `replays/win-m0018-g1.json`: win, match 18 game 1, 17 turns, battle_damage
- `replays/loss-m0000-g1.json`: loss, match 0 game 1, 20 turns, battle_damage
- `replays/loss-m0001-g1.json`: loss, match 1 game 1, 18 turns, battle_damage
- `replays/loss-m0002-g1.json`: loss, match 2 game 1, 17 turns, battle_damage
- `replays/loss-m0004-g1.json`: loss, match 4 game 1, 20 turns, battle_damage
- `replays/loss-m0005-g1.json`: loss, match 5 game 1, 16 turns, battle_damage
- `replays/loss-m0006-g1.json`: loss, match 6 game 1, 22 turns, battle_damage
- `replays/loss-m0007-g1.json`: loss, match 7 game 1, 16 turns, battle_damage
- `replays/loss-m0014-g1.json`: loss, match 14 game 1, 18 turns, battle_damage
- `replays/loss-m0015-g1.json`: loss, match 15 game 1, 18 turns, battle_damage
- `replays/loss-m0016-g1.json`: loss, match 16 game 1, 15 turns, battle_damage
- `replays/loss-m0019-g1.json`: loss, match 19 game 1, 19 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
