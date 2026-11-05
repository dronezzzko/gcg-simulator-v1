# red-green-zeon vs blue-white-federation

Deck under test (DUT) **red-green-zeon** against benchmark **blue-white-federation**: 1 BO1 matches, master seed 3, AI preset `fast` (agents `mcts-fast` / `mcts-fast`, factory `gcg_sim.ai.make_agent('mcts')`).

Reproduce: `gcg-sim benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt --matches 1 --seed 3 --format bo1 --ai-preset fast --replays 3`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |
| Games | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 0 | 0 | 0 | 0 | n/a (n=0) |
| On the draw | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |
| Game 1 | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |

Game length (turns): mean 13.0, median 13, p10 13, p90 13, range 13-13. Median by DUT result: win -, loss 13, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 100.0% [20.7%, 100.0%] (n=1); DUT win rate after a redraw 0.0% [0.0%, 79.3%] (n=1), after keeping n/a (n=0). Benchmark redraw rate 0.0% [0.0%, 79.3%] (n=1).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-030 Rick Dom | 4 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| GD01-031 Gelgoog | 4 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| GD01-035 Zaku Ⅱ | 3 | 1 (100.0%) | 1 | 2.5 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
| GD01-044 Kshatriya | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | 100.0% |
| GD01-059 Zee Zulu | 4 | 1 (100.0%) | 1 | 2.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
| GD01-092 M'Quve | 3 | 1 (100.0%) | 1 | 4.5 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
| GD01-093 Marida Cruz | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| GD01-115 Zeon Remnant Forces | 3 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| GD01-125 Zanzibar | 2 | 1 (100.0%) | 1 | 4.5 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | 100.0% |
| GD02-048 Zaku Ⅲ (Sleeves) | 3 | 1 (100.0%) | 1 | 3.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
| GD04-040 Schuzrum-Galluss | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | 100.0% |
| ST03-001 Sinanju | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| ST03-003 Geara Zulu | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| ST03-006 Char's Zaku Ⅱ | 3 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| ST03-008 Zaku Ⅱ | 4 | 1 (100.0%) | 1 | 3.5 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | 100.0% |
| ST03-010 Full Frontal | 2 | 1 (100.0%) | 0 | - | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | n/a (n=0) | 1 (100.0%) | n/a |
| ST03-012 Indignation | 2 | 1 (100.0%) | 1 | 4.5 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
| ST03-013 Close Combat | 3 | 1 (100.0%) | 1 | 5.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | 100.0% |

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

- `replays/loss-m0000-g1.json`: loss, match 0 game 1, 13 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
