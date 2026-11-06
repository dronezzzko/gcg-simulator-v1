# zeon's deck vs blue-white-federation

Deck under test (DUT) **zeon's deck** against benchmark **blue-white-federation**: 3 BO3 matches, master seed 42, AI preset `strong` (agents `test-random` / `test-random`, factory `random_agent.py:make_random_agent`).

Reproduce: `gcg-sim benchmark 'crates/gcg-runner/tests/vectors/records/decks/zeon'"'"'s deck.txt' examples/decks/blue-white-federation.txt --matches 3 --seed 42 --format bo3 --ai-preset strong --replays 3`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 3 | 2 | 1 | 0 | 66.7% [20.8%, 93.9%] (n=3) |
| Games | 7 | 4 | 3 | 0 | 57.1% [25.0%, 84.2%] (n=7) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 2 | 2 | 0 | 0 | 100.0% [34.2%, 100.0%] (n=2) |
| On the draw | 5 | 2 | 3 | 0 | 40.0% [11.8%, 76.9%] (n=5) |
| Game 1 | 3 | 2 | 1 | 0 | 66.7% [20.8%, 93.9%] (n=3) |
| Game 2 | 3 | 1 | 2 | 0 | 33.3% [6.1%, 79.2%] (n=3) |
| Game 3 | 1 | 1 | 0 | 0 | 100.0% [20.7%, 100.0%] (n=1) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 7 | 4 | 3 | 0 | 57.1% [25.0%, 84.2%] (n=7) |

Game length (turns): mean 19.0, median 16, p10 15, p90 29, range 15-29. Median by DUT result: win 16, loss 15, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 0.0% [0.0%, 35.4%] (n=7); DUT win rate after a redraw n/a (n=0), after keeping 57.1% [25.0%, 84.2%] (n=7). Benchmark redraw rate 42.9% [15.8%, 75.0%] (n=7).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-030 Rick Dom | 4 | 5 (71.4%) | 5 | 5.2 | 60.0% [23.1%, 88.2%] (n=5) | 50.0% [9.5%, 90.5%] (n=2) | 60.0% [23.1%, 88.2%] (n=5) | 0 (0.0%) | 0.0% |
| GD01-031 Gelgoog | 4 | 6 (85.7%) | 5 | 5.9 | 50.0% [18.8%, 81.2%] (n=6) | 100.0% [20.7%, 100.0%] (n=1) | 60.0% [23.1%, 88.2%] (n=5) | 1 (7.7%) | 0.0% |
| GD01-035 Zaku Ⅱ | 3 | 4 (57.1%) | 4 | 6.5 | 50.0% [15.0%, 85.0%] (n=4) | 66.7% [20.8%, 93.9%] (n=3) | 50.0% [15.0%, 85.0%] (n=4) | 1 (20.0%) | n/a |
| GD01-044 Kshatriya | 2 | 3 (42.9%) | 3 | 7.2 | 33.3% [6.1%, 79.2%] (n=3) | 75.0% [30.1%, 95.4%] (n=4) | 33.3% [6.1%, 79.2%] (n=3) | 1 (20.0%) | 0.0% |
| GD01-059 Zee Zulu | 4 | 6 (85.7%) | 6 | 5.9 | 66.7% [30.0%, 90.3%] (n=6) | 0.0% [0.0%, 79.3%] (n=1) | 66.7% [30.0%, 90.3%] (n=6) | 0 (0.0%) | 0.0% |
| GD01-092 M'Quve | 3 | 6 (85.7%) | 6 | 5.2 | 66.7% [30.0%, 90.3%] (n=6) | 0.0% [0.0%, 79.3%] (n=1) | 66.7% [30.0%, 90.3%] (n=6) | 0 (0.0%) | 0.0% |
| GD01-093 Marida Cruz | 2 | 2 (28.6%) | 2 | 4.5 | 50.0% [9.5%, 90.5%] (n=2) | 60.0% [23.1%, 88.2%] (n=5) | 50.0% [9.5%, 90.5%] (n=2) | 0 (0.0%) | n/a |
| GD01-115 Zeon Remnant Forces | 3 | 6 (85.7%) | 6 | 4.2 | 50.0% [18.8%, 81.2%] (n=6) | 100.0% [20.7%, 100.0%] (n=1) | 50.0% [18.8%, 81.2%] (n=6) | 0 (0.0%) | 0.0% |
| GD01-125 Zanzibar | 2 | 3 (42.9%) | 3 | 8.5 | 66.7% [20.8%, 93.9%] (n=3) | 50.0% [15.0%, 85.0%] (n=4) | 66.7% [20.8%, 93.9%] (n=3) | 0 (0.0%) | 0.0% |
| GD02-048 Zaku Ⅲ (Sleeves) | 3 | 5 (71.4%) | 5 | 7.7 | 60.0% [23.1%, 88.2%] (n=5) | 50.0% [9.5%, 90.5%] (n=2) | 60.0% [23.1%, 88.2%] (n=5) | 1 (14.3%) | 0.0% |
| GD04-040 Schuzrum-Galluss | 2 | 4 (57.1%) | 4 | 5.5 | 50.0% [15.0%, 85.0%] (n=4) | 66.7% [20.8%, 93.9%] (n=3) | 50.0% [15.0%, 85.0%] (n=4) | 0 (0.0%) | 0.0% |
| ST03-001 Sinanju | 2 | 4 (57.1%) | 4 | 8.8 | 75.0% [30.1%, 95.4%] (n=4) | 33.3% [6.1%, 79.2%] (n=3) | 75.0% [30.1%, 95.4%] (n=4) | 0 (0.0%) | 0.0% |
| ST03-003 Geara Zulu | 2 | 3 (42.9%) | 3 | 5.7 | 66.7% [20.8%, 93.9%] (n=3) | 50.0% [15.0%, 85.0%] (n=4) | 66.7% [20.8%, 93.9%] (n=3) | 1 (25.0%) | n/a |
| ST03-006 Char's Zaku Ⅱ | 3 | 6 (85.7%) | 4 | 8.3 | 50.0% [18.8%, 81.2%] (n=6) | 100.0% [20.7%, 100.0%] (n=1) | 75.0% [30.1%, 95.4%] (n=4) | 2 (22.2%) | 0.0% |
| ST03-008 Zaku Ⅱ | 4 | 6 (85.7%) | 6 | 4.9 | 66.7% [30.0%, 90.3%] (n=6) | 0.0% [0.0%, 79.3%] (n=1) | 66.7% [30.0%, 90.3%] (n=6) | 3 (25.0%) | 0.0% |
| ST03-010 Full Frontal | 2 | 2 (28.6%) | 1 | 6.0 | 50.0% [9.5%, 90.5%] (n=2) | 60.0% [23.1%, 88.2%] (n=5) | 100.0% [20.7%, 100.0%] (n=1) | 1 (33.3%) | 0.0% |
| ST03-012 Indignation | 2 | 3 (42.9%) | 3 | 7.3 | 66.7% [20.8%, 93.9%] (n=3) | 50.0% [15.0%, 85.0%] (n=4) | 66.7% [20.8%, 93.9%] (n=3) | 0 (0.0%) | n/a |
| ST03-013 Close Combat | 3 | 3 (42.9%) | 3 | 7.0 | 66.7% [20.8%, 93.9%] (n=3) | 50.0% [15.0%, 85.0%] (n=4) | 66.7% [20.8%, 93.9%] (n=3) | 0 (0.0%) | n/a |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.1.0; card data 31-ebd08a4b4f8c8898bc407498229839548fb911e9 (built 2026-11-02T14:57:40.351Z); Comprehensive Rules 1.10.0 (2026-10-16); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

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

- `replays/win-m0001-g1.json`: win, match 1 game 1, 15 turns, battle_damage
- `replays/win-m0002-g1.json`: win, match 2 game 1, 21 turns, battle_damage
- `replays/win-m0002-g2.json`: win, match 2 game 2, 16 turns, battle_damage
- `replays/loss-m0000-g1.json`: loss, match 0 game 1, 29 turns, battle_damage
- `replays/loss-m0000-g2.json`: loss, match 0 game 2, 15 turns, battle_damage
- `replays/loss-m0001-g2.json`: loss, match 1 game 2, 15 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
