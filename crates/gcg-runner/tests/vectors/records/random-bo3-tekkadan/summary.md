# purple-white-tekkadan vs blue-white-federation

Deck under test (DUT) **purple-white-tekkadan** against benchmark **blue-white-federation**: 12 BO3 matches, master seed 18446744073709551615, AI preset `fast` (agents `test-random` / `test-random`, factory `random_agent.py:make_random_agent`).

Reproduce: `gcg-sim benchmark examples/decks/purple-white-tekkadan.txt examples/decks/blue-white-federation.txt --matches 12 --seed 18446744073709551615 --format bo3 --ai-preset fast --replays 1`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 12 | 6 | 6 | 0 | 50.0% [25.4%, 74.6%] (n=12) |
| Games | 30 | 13 | 17 | 0 | 43.3% [27.4%, 60.8%] (n=30) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 16 | 9 | 7 | 0 | 56.2% [33.2%, 76.9%] (n=16) |
| On the draw | 14 | 4 | 10 | 0 | 28.6% [11.7%, 54.6%] (n=14) |
| Game 1 | 12 | 3 | 9 | 0 | 25.0% [8.9%, 53.2%] (n=12) |
| Game 2 | 12 | 5 | 7 | 0 | 41.7% [19.3%, 68.0%] (n=12) |
| Game 3 | 6 | 5 | 1 | 0 | 83.3% [43.6%, 97.0%] (n=6) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 30 | 13 | 17 | 0 | 43.3% [27.4%, 60.8%] (n=30) |

Game length (turns): mean 21.3, median 21, p10 15, p90 29, range 13-33. Median by DUT result: win 21, loss 21, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 70.0% [52.1%, 83.3%] (n=30); DUT win rate after a redraw 42.9% [24.5%, 63.5%] (n=21), after keeping 44.4% [18.9%, 73.3%] (n=9). Benchmark redraw rate 43.3% [27.4%, 60.8%] (n=30).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD02-054 Gundam Barbatos 1st Form | 4 | 18 (60.0%) | 17 | 6.4 | 44.4% [24.6%, 66.3%] (n=18) | 41.7% [19.3%, 68.0%] (n=12) | 41.2% [21.6%, 64.0%] (n=17) | 2 (7.1%) | 70.0% |
| GD02-058 Ryusei-Go (Graze Custom Ⅱ) | 2 | 13 (43.3%) | 10 | 6.5 | 53.8% [29.1%, 76.8%] (n=13) | 35.3% [17.3%, 58.7%] (n=17) | 40.0% [16.8%, 68.7%] (n=10) | 6 (37.5%) | 83.3% |
| GD02-114 It's Name is Ryusei-Go | 1 | 9 (30.0%) | 9 | 5.2 | 33.3% [12.1%, 64.6%] (n=9) | 47.6% [28.3%, 67.6%] (n=21) | 33.3% [12.1%, 64.6%] (n=9) | 0 (0.0%) | 100.0% |
| GD03-056 Gundam Barbatos Adapt | 3 | 22 (73.3%) | 17 | 7.6 | 45.5% [26.9%, 65.3%] (n=22) | 37.5% [13.7%, 69.4%] (n=8) | 52.9% [31.0%, 73.8%] (n=17) | 6 (20.0%) | 70.0% |
| GD03-066 Gundam Barbatos 5th Form (Ground Type) | 2 | 16 (53.3%) | 15 | 7.4 | 62.5% [38.6%, 81.5%] (n=16) | 21.4% [7.6%, 47.6%] (n=14) | 60.0% [35.7%, 80.2%] (n=15) | 1 (5.9%) | 57.1% |
| GD05-058 Shiden Custom (Ryusei-Go) | 2 | 21 (70.0%) | 20 | 6.3 | 42.9% [24.5%, 63.5%] (n=21) | 44.4% [18.9%, 73.3%] (n=9) | 45.0% [25.8%, 65.8%] (n=20) | 1 (3.7%) | 80.0% |
| GD05-060 Gundam Flauros (Ryusei-Go) | 2 | 14 (46.7%) | 12 | 6.8 | 35.7% [16.3%, 61.2%] (n=14) | 50.0% [28.0%, 72.0%] (n=16) | 41.7% [19.3%, 68.0%] (n=12) | 2 (12.5%) | 66.7% |
| GD05-117 Become a Shield | 3 | 23 (76.7%) | 23 | 6.1 | 43.5% [25.6%, 63.2%] (n=23) | 42.9% [15.8%, 75.0%] (n=7) | 43.5% [25.6%, 63.2%] (n=23) | 0 (0.0%) | 90.0% |
| ST01-014 Unforeseen Incident | 3 | 23 (76.7%) | 21 | 5.4 | 39.1% [22.2%, 59.2%] (n=23) | 57.1% [25.0%, 84.2%] (n=7) | 33.3% [17.2%, 54.6%] (n=21) | 3 (10.7%) | 100.0% |
| ST05-001 Gundam Barbatos 4th Form | 3 | 23 (76.7%) | 18 | 8.7 | 43.5% [25.6%, 63.2%] (n=23) | 42.9% [15.8%, 75.0%] (n=7) | 44.4% [24.6%, 66.3%] (n=18) | 8 (22.2%) | 61.5% |
| ST05-002 Gundam Barbatos 2nd Form | 4 | 27 (90.0%) | 23 | 6.7 | 44.4% [27.6%, 62.7%] (n=27) | 33.3% [6.1%, 79.2%] (n=3) | 43.5% [25.6%, 63.2%] (n=23) | 7 (14.9%) | 77.8% |
| ST05-003 CGS Mobile Worker | 4 | 28 (93.3%) | 27 | 5.0 | 42.9% [26.5%, 60.9%] (n=28) | 50.0% [9.5%, 90.5%] (n=2) | 44.4% [27.6%, 62.7%] (n=27) | 5 (10.0%) | 50.0% |
| ST05-004 Graze Custom | 4 | 24 (80.0%) | 22 | 5.2 | 50.0% [31.4%, 68.6%] (n=24) | 16.7% [3.0%, 56.4%] (n=6) | 50.0% [30.7%, 69.3%] (n=22) | 2 (4.9%) | 86.7% |
| ST05-007 McGillis' Schwalbe Graze | 2 | 11 (36.7%) | 8 | 7.6 | 45.5% [21.3%, 72.0%] (n=11) | 42.1% [23.1%, 63.7%] (n=19) | 50.0% [21.5%, 78.5%] (n=8) | 3 (25.0%) | 66.7% |
| ST05-010 Mikazuki Augus | 4 | 23 (76.7%) | 21 | 6.9 | 39.1% [22.2%, 59.2%] (n=23) | 57.1% [25.0%, 84.2%] (n=7) | 42.9% [24.5%, 63.5%] (n=21) | 10 (22.7%) | 42.9% |
| ST05-011 Akihiro Altland | 3 | 23 (76.7%) | 23 | 7.2 | 39.1% [22.2%, 59.2%] (n=23) | 57.1% [25.0%, 84.2%] (n=7) | 39.1% [22.2%, 59.2%] (n=23) | 3 (8.6%) | 80.0% |
| ST05-012 McGillis Fareed | 2 | 16 (53.3%) | 14 | 6.7 | 43.8% [23.1%, 66.8%] (n=16) | 42.9% [21.4%, 67.4%] (n=14) | 50.0% [26.8%, 73.2%] (n=14) | 4 (18.2%) | 50.0% |
| ST05-015 Isaribi | 2 | 19 (63.3%) | 18 | 6.1 | 42.1% [23.1%, 63.7%] (n=19) | 45.5% [21.3%, 72.0%] (n=11) | 38.9% [20.3%, 61.4%] (n=18) | 1 (4.0%) | 25.0% |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.1.0; card data 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42 (built 2026-10-12T14:37:38.154Z); Comprehensive Rules 1.9.0 (2026-09-11); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

## Conflict resolutions affecting these decks

- `ruling:GD01-001:Q119` (GD01-001 Gundam; benchmark): 'All your (X) Units' and 'when one of your (X) Units is deployed' include the source card itself; constant effects (including trait grants) apply before trigger conditions of the entering card are checked.
- `divergent:GD01-087` (GD01-087 Sayla Mass; benchmark): Use GD01-087. The printings differ only in parenthesized reminder text.
- `ambiguous:GD03-001:when-effect-destroys-enemy` (GD03-001 Gundam NT-1; benchmark): Draw only if the chosen Unit was destroyed and is no longer in the battle area right after this effect's damage and rules management.
- `ruling:GD03-001:Q209` (GD03-001 Gundam NT-1; benchmark): 'When this effect destroys an enemy Unit' is checked right after this effect's damage is dealt (rules management); later destruction does not count.
- `ruling:GD03-056:Q230` (GD03-056 Gundam Barbatos Adapt; deck_under_test): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `data-null:BASE:ap` (GD04-122 Jaburo; benchmark): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:GD05-060:Q372` (GD05-060 Gundam Flauros (Ryusei-Go); deck_under_test): '【A】/【B】' gives the effect both timings; each triggers independently (no shared once-per-turn).
- `ruling:GD05-060:Q373` (GD05-060 Gundam Flauros (Ryusei-Go); deck_under_test): '【A】/【B】' gives the effect both timings; each triggers independently (no shared once-per-turn).
- `ruling:GD05-117:Q412` (GD05-117 Become a Shield; deck_under_test): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `divergent:R-001` (R-001 Resource; benchmark, deck_under_test): Use R-001_p7: a Resource card has no AP or HP. The Edition Beta record R-001_p4 (AP3/HP1) is a data error.
- `ruling:ST01-001:Q113` (ST01-001 Gundam; benchmark): 'All your (X) Units' and 'when one of your (X) Units is deployed' include the source card itself; constant effects (including trait grants) apply before trigger conditions of the entering card are checked.
- `ambiguous:suppression-with-base` (ST05-001 Gundam Barbatos 4th Form; deck_under_test): With a Base in the shield area the attack damages only the Base (<Suppression> does nothing, excess is lost). Without a Base, <Suppression> deals the attacker's battle damage to the top two Shields simultaneously (one if only one remains, 13-1-7-3).
- `divergent:ST05-001` (ST05-001 Gundam Barbatos 4th Form; deck_under_test): Use ST05-001. The SC01 reprint (_p3) lists the effects in the other order, which changes nothing.
- `data-null:UNIT:ap` (ST05-003 CGS Mobile Worker; deck_under_test): These Units have no AP value; treat AP as 0 (they deal no battle damage, 5-5-5).
- `ruling:ST05-010:Q168` (ST05-010 Mikazuki Augus; deck_under_test): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `ruling:ST05-011:Q169` (ST05-011 Akihiro Altland; deck_under_test): 'When this Unit destroys an enemy … with (battle) damage' triggers even if this Unit is destroyed simultaneously; the pending trigger resolves after both Units are in the trash (parts that need this Unit on the field do nothing).
- `data-null:BASE:ap` (ST05-015 Isaribi; deck_under_test): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:ST05-015:Q170` (ST05-015 Isaribi; deck_under_test): Conditions and target restrictions of triggered/activated/command effects are evaluated once, when resolved; later changes neither undo the result nor retrigger it.
- `data-null:BASE:ap` (ST08-015 Davao; benchmark): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).

## Replays

- `replays/win-m0000-g3.json`: win, match 0 game 3, 21 turns, battle_damage
- `replays/loss-m0002-g1.json`: loss, match 2 game 1, 21 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
