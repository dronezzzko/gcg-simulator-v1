# blue-white-federation vs red-green-zeon

Deck under test (DUT) **blue-white-federation** against benchmark **red-green-zeon**: 1 BO3 matches, master seed 7, AI preset `fast` (agents `mcts-fast` / `mcts-fast`, factory `gcg_sim.ai.make_agent('mcts')`).

Reproduce: `gcg-sim benchmark examples/decks/blue-white-federation.txt examples/decks/red-green-zeon.txt --matches 1 --seed 7 --format bo3 --ai-preset fast --replays 1 --decision-log`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |
| Games | 3 | 1 | 2 | 0 | 33.3% [6.1%, 79.2%] (n=3) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 1 | 1 | 0 | 0 | 100.0% [20.7%, 100.0%] (n=1) |
| On the draw | 2 | 0 | 2 | 0 | 0.0% [0.0%, 65.8%] (n=2) |
| Game 1 | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |
| Game 2 | 1 | 1 | 0 | 0 | 100.0% [20.7%, 100.0%] (n=1) |
| Game 3 | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 3 | 1 | 2 | 0 | 33.3% [6.1%, 79.2%] (n=3) |

Game length (turns): mean 15.0, median 15, p10 13, p90 17, range 13-17. Median by DUT result: win 13, loss 15, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 33.3% [6.1%, 79.2%] (n=3); DUT win rate after a redraw 0.0% [0.0%, 79.3%] (n=1), after keeping 50.0% [9.5%, 90.5%] (n=2). Benchmark redraw rate 33.3% [6.1%, 79.2%] (n=3).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-001 Gundam | 2 | 3 (100.0%) | 3 | 6.0 | 33.3% [6.1%, 79.2%] (n=3) | n/a (n=0) | 33.3% [6.1%, 79.2%] (n=3) | 0 (0.0%) | n/a |
| GD01-004 Guncannon | 4 | 2 (66.7%) | 2 | 3.0 | 50.0% [9.5%, 90.5%] (n=2) | 0.0% [0.0%, 79.3%] (n=1) | 50.0% [9.5%, 90.5%] (n=2) | 0 (0.0%) | 50.0% |
| GD01-008 Guntank | 4 | 3 (100.0%) | 3 | 3.6 | 33.3% [6.1%, 79.2%] (n=3) | n/a (n=0) | 33.3% [6.1%, 79.2%] (n=3) | 0 (0.0%) | 0.0% |
| GD01-087 Sayla Mass | 2 | 2 (66.7%) | 2 | 3.5 | 0.0% [0.0%, 65.8%] (n=2) | 100.0% [20.7%, 100.0%] (n=1) | 0.0% [0.0%, 65.8%] (n=2) | 0 (0.0%) | 50.0% |
| GD01-099 Intercept Orders | 2 | 2 (66.7%) | 2 | 5.3 | 0.0% [0.0%, 65.8%] (n=2) | 100.0% [20.7%, 100.0%] (n=1) | 0.0% [0.0%, 65.8%] (n=2) | 0 (0.0%) | 100.0% |
| GD03-001 Gundam NT-1 | 2 | 1 (33.3%) | 0 | - | 100.0% [20.7%, 100.0%] (n=1) | 0.0% [0.0%, 65.8%] (n=2) | n/a (n=0) | 1 (100.0%) | n/a |
| GD04-008 Gundam | 2 | 2 (66.7%) | 2 | 5.7 | 50.0% [9.5%, 90.5%] (n=2) | 0.0% [0.0%, 79.3%] (n=1) | 50.0% [9.5%, 90.5%] (n=2) | 0 (0.0%) | 0.0% |
| GD04-012 Core Booster (005) & Core Booster (006) | 2 | 1 (33.3%) | 1 | 3.0 | 100.0% [20.7%, 100.0%] (n=1) | 0.0% [0.0%, 65.8%] (n=2) | 100.0% [20.7%, 100.0%] (n=1) | 0 (0.0%) | n/a |
| GD04-084 Sleggar Law | 2 | 2 (66.7%) | 2 | 3.0 | 50.0% [9.5%, 90.5%] (n=2) | 0.0% [0.0%, 79.3%] (n=1) | 50.0% [9.5%, 90.5%] (n=2) | 0 (0.0%) | n/a |
| GD04-122 Jaburo | 2 | 1 (33.3%) | 1 | 6.0 | 100.0% [20.7%, 100.0%] (n=1) | 0.0% [0.0%, 65.8%] (n=2) | 100.0% [20.7%, 100.0%] (n=1) | 0 (0.0%) | n/a |
| ST01-001 Gundam | 4 | 2 (66.7%) | 2 | 6.0 | 0.0% [0.0%, 65.8%] (n=2) | 100.0% [20.7%, 100.0%] (n=1) | 0.0% [0.0%, 65.8%] (n=2) | 0 (0.0%) | 0.0% |
| ST01-002 Gundam (MA Form) | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 33.3% [6.1%, 79.2%] (n=3) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-004 Guntank | 3 | 2 (66.7%) | 2 | 2.5 | 0.0% [0.0%, 65.8%] (n=2) | 100.0% [20.7%, 100.0%] (n=1) | 0.0% [0.0%, 65.8%] (n=2) | 0 (0.0%) | 0.0% |
| ST01-005 GM | 3 | 3 (100.0%) | 3 | 2.5 | 33.3% [6.1%, 79.2%] (n=3) | n/a (n=0) | 33.3% [6.1%, 79.2%] (n=3) | 0 (0.0%) | 0.0% |
| ST01-010 Amuro Ray | 4 | 2 (66.7%) | 1 | 4.0 | 50.0% [9.5%, 90.5%] (n=2) | 0.0% [0.0%, 79.3%] (n=1) | 0.0% [0.0%, 79.3%] (n=1) | 1 (50.0%) | 50.0% |
| ST01-012 Thoroughly Damaged | 2 | 1 (33.3%) | 0 | - | 100.0% [20.7%, 100.0%] (n=1) | 0.0% [0.0%, 65.8%] (n=2) | n/a (n=0) | 1 (100.0%) | n/a |
| ST01-014 Unforeseen Incident | 3 | 1 (33.3%) | 1 | 4.0 | 0.0% [0.0%, 79.3%] (n=1) | 50.0% [9.5%, 90.5%] (n=2) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
| ST04-013 Hawk of Endymion | 3 | 3 (100.0%) | 3 | 4.0 | 33.3% [6.1%, 79.2%] (n=3) | n/a (n=0) | 33.3% [6.1%, 79.2%] (n=3) | 0 (0.0%) | 50.0% |
| ST08-015 Davao | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 33.3% [6.1%, 79.2%] (n=3) | n/a (n=0) | 0 (n/a) | n/a |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.1.0; card data 28-ad9da1b22fc2abd56c1176a5e64b12e6ce21ea42 (built 2026-10-12T14:37:38.154Z); Comprehensive Rules 1.9.0 (2026-09-11); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

## Conflict resolutions affecting these decks

- `ruling:GD01-001:Q119` (GD01-001 Gundam; deck_under_test): 'All your (X) Units' and 'when one of your (X) Units is deployed' include the source card itself; constant effects (including trait grants) apply before trigger conditions of the entering card are checked.
- `ambiguous:breach-reminder` (GD01-030 Rick Dom; benchmark): Apply 13-1-2-1 for every card: <Breach> triggers when, during its controller's turn, the Unit destroys an enemy Unit with battle damage (including battles begun by effects, Q347); reminder wording is ignored.
- `divergent:GD01-030` (GD01-030 Rick Dom; benchmark): Use GD01-030. The printings differ only in parenthesized reminder text.
- `ruling:GD01-059:Q135` (GD01-059 Zee Zulu; benchmark): Conditions and target restrictions of triggered/activated/command effects are evaluated once, when resolved; later changes neither undo the result nor retrigger it.
- `divergent:GD01-087` (GD01-087 Sayla Mass; deck_under_test): Use GD01-087. The printings differ only in parenthesized reminder text.
- `ambiguous:breach-reminder` (GD01-092 M'Quve; benchmark): Apply 13-1-2-1 for every card: <Breach> triggers when, during its controller's turn, the Unit destroys an enemy Unit with battle damage (including battles begun by effects, Q347); reminder wording is ignored.
- `data-null:BASE:ap` (GD01-125 Zanzibar; benchmark): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `ruling:GD01-125:Q157` (GD01-125 Zanzibar; benchmark): A card deployed, paired or activated by an effect pays no cost and needs no Lv. check unless the text says 'Pay its cost'; its own 【Deploy】/【When Paired】/【When Linked】 still trigger.
- `ambiguous:GD03-001:when-effect-destroys-enemy` (GD03-001 Gundam NT-1; deck_under_test): Draw only if the chosen Unit was destroyed and is no longer in the battle area right after this effect's damage and rules management.
- `ruling:GD03-001:Q209` (GD03-001 Gundam NT-1; deck_under_test): 'When this effect destroys an enemy Unit' is checked right after this effect's damage is dealt (rules management); later destruction does not count.
- `data-null:BASE:ap` (GD04-122 Jaburo; deck_under_test): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).
- `divergent:R-001` (R-001 Resource; benchmark, deck_under_test): Use R-001_p7: a Resource card has no AP or HP. The Edition Beta record R-001_p4 (AP3/HP1) is a data error.
- `ruling:ST01-001:Q113` (ST01-001 Gundam; deck_under_test): 'All your (X) Units' and 'when one of your (X) Units is deployed' include the source card itself; constant effects (including trait grants) apply before trigger conditions of the entering card are checked.
- `divergent:ST03-001` (ST03-001 Sinanju; benchmark): Use ST03-001. The SC01 reprint (_p2) lists the two effects in the other order, which changes nothing: they are separate effects (ruling Q164).
- `ruling:ST03-001:Q164` (ST03-001 Sinanju; benchmark): An effect gated by 【During Pair】/【During Link】/'while …' is not possessed while the gate is closed (so it does not satisfy 'Unit with a 【Destroyed】 effect' or 'Unit card with <Keyword>'); each 【During Pair】 prefix governs only its own effect.
- `ruling:ST03-010:Q145` (ST03-010 Full Frontal; benchmark): A card deployed, paired or activated by an effect pays no cost and needs no Lv. check unless the text says 'Pay its cost'; its own 【Deploy】/【When Paired】/【When Linked】 still trigger.
- `data-null:BASE:ap` (ST08-015 Davao; deck_under_test): Bases have no printed AP; treat AP as 0 (see rules-internal:base-ap).

## Replays

- `replays/win-m0000-g2.json`: win, match 0 game 2, 13 turns, battle_damage
- `replays/loss-m0000-g3.json`: loss, match 0 game 3, 15 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
