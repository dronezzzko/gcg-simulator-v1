# blue-white-federation vs purple-white-tekkadan

Deck under test (DUT) **blue-white-federation** against benchmark **purple-white-tekkadan**: 20 BO1 matches, master seed 22710602, AI preset `standard` (agents `search-standard` / `search-standard`, factory `gcg-search::make_agent('standard')`).

Search: `gcg-search`, preset `standard` (reference strong mapping), 1024 iterations per option (min 2560, max 10240, auxiliary apply cap 16 per iteration), mechanisms: `adaptive_horizon`, `capped_rollout_continuation`, `engine_error_leaf`, `exclude_capped_play_draw`, `kill_aware_targeting`, `tactical_solver`, `tree_lethal_select`, weights `ROLE_WEIGHTS`.

Reproduce: `gcg-sim benchmark examples/decks/blue-white-federation.txt examples/decks/purple-white-tekkadan.txt --matches 20 --seed 22710602 --format bo1 --ai-preset standard --replays 20`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 20 | 12 | 8 | 0 | 60.0% [38.7%, 78.1%] (n=20) |
| Games | 20 | 12 | 8 | 0 | 60.0% [38.7%, 78.1%] (n=20) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 11 | 7 | 4 | 0 | 63.6% [35.4%, 84.8%] (n=11) |
| On the draw | 9 | 5 | 4 | 0 | 55.6% [26.7%, 81.1%] (n=9) |
| Game 1 | 20 | 12 | 8 | 0 | 60.0% [38.7%, 78.1%] (n=20) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 20 | 12 | 8 | 0 | 60.0% [38.7%, 78.1%] (n=20) |

Game length (turns): mean 18.2, median 17, p10 12, p90 22, range 12-28. Median by DUT result: win 17, loss 17, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 25.0% [11.2%, 46.9%] (n=20); DUT win rate after a redraw 80.0% [37.6%, 96.4%] (n=5), after keeping 53.3% [30.1%, 75.2%] (n=15). Benchmark redraw rate 40.0% [21.9%, 61.3%] (n=20).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-001 Gundam | 2 | 13 (65.0%) | 13 | 6.5 | 69.2% [42.4%, 87.3%] (n=13) | 42.9% [15.8%, 75.0%] (n=7) | 69.2% [42.4%, 87.3%] (n=13) | 1 (6.2%) | 25.0% |
| GD01-004 Guncannon | 4 | 18 (90.0%) | 18 | 5.7 | 66.7% [43.7%, 83.7%] (n=18) | 0.0% [0.0%, 65.8%] (n=2) | 66.7% [43.7%, 83.7%] (n=18) | 0 (0.0%) | 0.0% |
| GD01-008 Guntank | 4 | 16 (80.0%) | 16 | 4.1 | 56.2% [33.2%, 76.9%] (n=16) | 75.0% [30.1%, 95.4%] (n=4) | 56.2% [33.2%, 76.9%] (n=16) | 1 (5.6%) | 33.3% |
| GD01-087 Sayla Mass | 2 | 11 (55.0%) | 10 | 5.7 | 45.5% [21.3%, 72.0%] (n=11) | 77.8% [45.3%, 93.7%] (n=9) | 50.0% [23.7%, 76.3%] (n=10) | 2 (15.4%) | 33.3% |
| GD01-099 Intercept Orders | 2 | 9 (45.0%) | 8 | 6.9 | 44.4% [18.9%, 73.3%] (n=9) | 72.7% [43.4%, 90.3%] (n=11) | 37.5% [13.7%, 69.4%] (n=8) | 2 (16.7%) | 20.0% |
| GD03-001 Gundam NT-1 | 2 | 7 (35.0%) | 6 | 5.5 | 71.4% [35.9%, 91.8%] (n=7) | 53.8% [29.1%, 76.8%] (n=13) | 66.7% [30.0%, 90.3%] (n=6) | 1 (14.3%) | 50.0% |
| GD04-008 Gundam | 2 | 12 (60.0%) | 11 | 5.9 | 75.0% [46.8%, 91.1%] (n=12) | 37.5% [13.7%, 69.4%] (n=8) | 72.7% [43.4%, 90.3%] (n=11) | 1 (6.2%) | 25.0% |
| GD04-012 Core Booster (005) & Core Booster (006) | 2 | 13 (65.0%) | 13 | 6.4 | 76.9% [49.7%, 91.8%] (n=13) | 28.6% [8.2%, 64.1%] (n=7) | 76.9% [49.7%, 91.8%] (n=13) | 0 (0.0%) | 25.0% |
| GD04-084 Sleggar Law | 2 | 15 (75.0%) | 14 | 5.1 | 60.0% [35.7%, 80.2%] (n=15) | 60.0% [23.1%, 88.2%] (n=5) | 57.1% [32.6%, 78.6%] (n=14) | 3 (15.0%) | 28.6% |
| GD04-122 Jaburo | 2 | 16 (80.0%) | 16 | 6.7 | 56.2% [33.2%, 76.9%] (n=16) | 75.0% [30.1%, 95.4%] (n=4) | 56.2% [33.2%, 76.9%] (n=16) | 2 (10.0%) | 0.0% |
| ST01-001 Gundam | 4 | 15 (75.0%) | 15 | 6.0 | 66.7% [41.7%, 84.8%] (n=15) | 40.0% [11.8%, 76.9%] (n=5) | 66.7% [41.7%, 84.8%] (n=15) | 1 (4.3%) | 50.0% |
| ST01-002 Gundam (MA Form) | 2 | 10 (50.0%) | 10 | 6.0 | 50.0% [23.7%, 76.3%] (n=10) | 70.0% [39.7%, 89.2%] (n=10) | 50.0% [23.7%, 76.3%] (n=10) | 0 (0.0%) | 42.9% |
| ST01-004 Guntank | 3 | 15 (75.0%) | 15 | 4.4 | 53.3% [30.1%, 75.2%] (n=15) | 80.0% [37.6%, 96.4%] (n=5) | 53.3% [30.1%, 75.2%] (n=15) | 1 (4.3%) | 0.0% |
| ST01-005 GM | 3 | 15 (75.0%) | 15 | 3.7 | 60.0% [35.7%, 80.2%] (n=15) | 60.0% [23.1%, 88.2%] (n=5) | 60.0% [35.7%, 80.2%] (n=15) | 0 (0.0%) | 0.0% |
| ST01-010 Amuro Ray | 4 | 17 (85.0%) | 17 | 5.8 | 64.7% [41.3%, 82.7%] (n=17) | 33.3% [6.1%, 79.2%] (n=3) | 64.7% [41.3%, 82.7%] (n=17) | 2 (6.7%) | 20.0% |
| ST01-012 Thoroughly Damaged | 2 | 12 (60.0%) | 12 | 5.7 | 50.0% [25.4%, 74.6%] (n=12) | 75.0% [40.9%, 92.9%] (n=8) | 50.0% [25.4%, 74.6%] (n=12) | 1 (6.2%) | 28.6% |
| ST01-014 Unforeseen Incident | 3 | 16 (80.0%) | 16 | 5.3 | 62.5% [38.6%, 81.5%] (n=16) | 50.0% [15.0%, 85.0%] (n=4) | 62.5% [38.6%, 81.5%] (n=16) | 0 (0.0%) | 50.0% |
| ST04-013 Hawk of Endymion | 3 | 14 (70.0%) | 14 | 6.7 | 42.9% [21.4%, 67.4%] (n=14) | 100.0% [61.0%, 100.0%] (n=6) | 42.9% [21.4%, 67.4%] (n=14) | 0 (0.0%) | 50.0% |
| ST08-015 Davao | 2 | 9 (45.0%) | 9 | 6.8 | 55.6% [26.7%, 81.1%] (n=9) | 63.6% [35.4%, 84.8%] (n=11) | 55.6% [26.7%, 81.1%] (n=9) | 1 (8.3%) | 50.0% |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.2.0; card data 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42 (built 2026-10-12T14:37:38.154Z); Comprehensive Rules 1.9.0 (2026-09-11); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

## Conflict resolutions affecting these decks

- `ruling:GD01-001:Q119` (GD01-001 Gundam; deck_under_test): 'All your (X) Units' and 'when one of your (X) Units is deployed' include the source card itself; constant effects (including trait grants) apply before trigger conditions of the entering card are checked.
- `divergent:GD01-087` (GD01-087 Sayla Mass; deck_under_test): Use GD01-087. The printings differ only in parenthesized reminder text.
- `ambiguous:GD03-001:when-effect-destroys-enemy` (GD03-001 Gundam NT-1; deck_under_test): Draw only if the chosen Unit was destroyed and is no longer in the battle area right after this effect's damage and rules management.
- `ruling:GD03-001:Q209` (GD03-001 Gundam NT-1; deck_under_test): 'When this effect destroys an enemy Unit' is checked right after this effect's damage is dealt (rules management); later destruction does not count.
- `ruling:GD03-056:Q230` (GD03-056 Gundam Barbatos Adapt; benchmark): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `data-null:BASE:ap` (GD04-122 Jaburo; deck_under_test): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:GD05-060:Q372` (GD05-060 Gundam Flauros (Ryusei-Go); benchmark): '【A】/【B】' gives the effect both timings; each triggers independently (no shared once-per-turn).
- `ruling:GD05-060:Q373` (GD05-060 Gundam Flauros (Ryusei-Go); benchmark): '【A】/【B】' gives the effect both timings; each triggers independently (no shared once-per-turn).
- `ruling:GD05-117:Q412` (GD05-117 Become a Shield; benchmark): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `divergent:R-001` (R-001 Resource; benchmark, deck_under_test): Use R-001_p7: a Resource card has no AP or HP. The Edition Beta record R-001_p4 (AP3/HP1) is a data error.
- `ruling:ST01-001:Q113` (ST01-001 Gundam; deck_under_test): 'All your (X) Units' and 'when one of your (X) Units is deployed' include the source card itself; constant effects (including trait grants) apply before trigger conditions of the entering card are checked.
- `ambiguous:suppression-with-base` (ST05-001 Gundam Barbatos 4th Form; benchmark): With a Base in the shield area the attack damages only the Base (<Suppression> does nothing, excess is lost). Without a Base, <Suppression> deals the attacker's battle damage to the top two Shields simultaneously (one if only one remains, 13-1-7-3).
- `divergent:ST05-001` (ST05-001 Gundam Barbatos 4th Form; benchmark): Use ST05-001. The SC01 reprint (_p3) lists the effects in the other order, which changes nothing.
- `data-null:UNIT:ap` (ST05-003 CGS Mobile Worker; benchmark): These Units have no AP value; treat AP as 0 (they deal no battle damage, 5-5-5).
- `ruling:ST05-010:Q168` (ST05-010 Mikazuki Augus; benchmark): 'Choose 1 X and 1 Y' activates (or the Command can be played) only if both targets can be chosen; it never resolves with just one.
- `ruling:ST05-011:Q169` (ST05-011 Akihiro Altland; benchmark): 'When this Unit destroys an enemy … with (battle) damage' triggers even if this Unit is destroyed simultaneously; the pending trigger resolves after both Units are in the trash (parts that need this Unit on the field do nothing).
- `data-null:BASE:ap` (ST05-015 Isaribi; benchmark): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:ST05-015:Q170` (ST05-015 Isaribi; benchmark): Conditions and target restrictions of triggered/activated/command effects are evaluated once, when resolved; later changes neither undo the result nor retrigger it.
- `data-null:BASE:ap` (ST08-015 Davao; deck_under_test): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).

## Replays

- `replays/win-m0001-g1.json`: win, match 1 game 1, 12 turns, battle_damage
- `replays/win-m0002-g1.json`: win, match 2 game 1, 18 turns, battle_damage
- `replays/win-m0003-g1.json`: win, match 3 game 1, 17 turns, battle_damage
- `replays/win-m0006-g1.json`: win, match 6 game 1, 22 turns, battle_damage
- `replays/win-m0008-g1.json`: win, match 8 game 1, 15 turns, battle_damage
- `replays/win-m0009-g1.json`: win, match 9 game 1, 17 turns, battle_damage
- `replays/win-m0011-g1.json`: win, match 11 game 1, 13 turns, battle_damage
- `replays/win-m0012-g1.json`: win, match 12 game 1, 27 turns, battle_damage
- `replays/win-m0013-g1.json`: win, match 13 game 1, 19 turns, battle_damage
- `replays/win-m0016-g1.json`: win, match 16 game 1, 21 turns, battle_damage
- `replays/win-m0017-g1.json`: win, match 17 game 1, 12 turns, battle_damage
- `replays/win-m0019-g1.json`: win, match 19 game 1, 28 turns, battle_damage
- `replays/loss-m0000-g1.json`: loss, match 0 game 1, 15 turns, battle_damage
- `replays/loss-m0004-g1.json`: loss, match 4 game 1, 16 turns, battle_damage
- `replays/loss-m0005-g1.json`: loss, match 5 game 1, 21 turns, battle_damage
- `replays/loss-m0007-g1.json`: loss, match 7 game 1, 17 turns, battle_damage
- `replays/loss-m0010-g1.json`: loss, match 10 game 1, 20 turns, battle_damage
- `replays/loss-m0014-g1.json`: loss, match 14 game 1, 17 turns, battle_damage
- `replays/loss-m0015-g1.json`: loss, match 15 game 1, 16 turns, battle_damage
- `replays/loss-m0018-g1.json`: loss, match 18 game 1, 22 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
