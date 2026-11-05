# blue-white-federation vs red-green-zeon

Deck under test (DUT) **blue-white-federation** against benchmark **red-green-zeon**: 60 BO3 matches, master seed 3, AI preset `standard` (agents `test-random` / `test-random`, factory `random_agent.py:make_random_agent`).

Reproduce: `gcg-sim benchmark examples/decks/blue-white-federation.txt examples/decks/red-green-zeon.txt --matches 60 --seed 3 --format bo3 --ai-preset standard --replays 2`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 60 | 20 | 40 | 0 | 33.3% [22.7%, 45.9%] (n=60) |
| Games | 139 | 52 | 87 | 0 | 37.4% [29.8%, 45.7%] (n=139) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 67 | 27 | 40 | 0 | 40.3% [29.4%, 52.3%] (n=67) |
| On the draw | 72 | 25 | 47 | 0 | 34.7% [24.8%, 46.2%] (n=72) |
| Game 1 | 60 | 24 | 36 | 0 | 40.0% [28.6%, 52.6%] (n=60) |
| Game 2 | 60 | 21 | 39 | 0 | 35.0% [24.2%, 47.6%] (n=60) |
| Game 3 | 19 | 7 | 12 | 0 | 36.8% [19.1%, 59.0%] (n=19) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 139 | 52 | 87 | 0 | 37.4% [29.8%, 45.7%] (n=139) |

Game length (turns): mean 19.2, median 19, p10 14, p90 25, range 10-41. Median by DUT result: win 20, loss 18, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 47.5% [39.4%, 55.7%] (n=139); DUT win rate after a redraw 37.9% [27.1%, 49.9%] (n=66), after keeping 37.0% [26.8%, 48.5%] (n=73). Benchmark redraw rate 44.6% [36.6%, 52.9%] (n=139).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-001 Gundam | 2 | 81 (58.3%) | 70 | 6.7 | 37.0% [27.3%, 47.9%] (n=81) | 37.9% [26.6%, 50.8%] (n=58) | 38.6% [28.0%, 50.3%] (n=70) | 21 (21.0%) | 54.5% |
| GD01-004 Guncannon | 4 | 110 (79.1%) | 95 | 5.8 | 38.2% [29.6%, 47.5%] (n=110) | 34.5% [19.9%, 52.7%] (n=29) | 42.1% [32.7%, 52.2%] (n=95) | 34 (19.4%) | 49.1% |
| GD01-008 Guntank | 4 | 111 (79.9%) | 106 | 4.9 | 42.3% [33.6%, 51.6%] (n=111) | 17.9% [7.9%, 35.6%] (n=28) | 42.5% [33.5%, 52.0%] (n=106) | 17 (9.9%) | 47.9% |
| GD01-087 Sayla Mass | 2 | 79 (56.8%) | 67 | 5.9 | 44.3% [33.9%, 55.3%] (n=79) | 28.3% [18.5%, 40.8%] (n=60) | 49.3% [37.7%, 60.9%] (n=67) | 21 (21.6%) | 41.9% |
| GD01-099 Intercept Orders | 2 | 80 (57.6%) | 79 | 5.6 | 36.2% [26.6%, 47.2%] (n=80) | 39.0% [27.6%, 51.7%] (n=59) | 36.7% [26.9%, 47.7%] (n=79) | 1 (1.1%) | 56.7% |
| GD03-001 Gundam NT-1 | 2 | 77 (55.4%) | 63 | 6.9 | 42.9% [32.4%, 54.0%] (n=77) | 30.6% [20.6%, 43.0%] (n=62) | 49.2% [37.3%, 61.2%] (n=63) | 20 (22.2%) | 50.0% |
| GD04-008 Gundam | 2 | 79 (56.8%) | 60 | 7.0 | 39.2% [29.2%, 50.3%] (n=79) | 35.0% [24.2%, 47.6%] (n=60) | 45.0% [33.1%, 57.5%] (n=60) | 26 (28.3%) | 55.6% |
| GD04-012 Core Booster (005) & Core Booster (006) | 2 | 68 (48.9%) | 62 | 5.5 | 50.0% [38.4%, 61.6%] (n=68) | 25.4% [16.7%, 36.6%] (n=71) | 54.8% [42.5%, 66.6%] (n=62) | 10 (12.7%) | 50.0% |
| GD04-084 Sleggar Law | 2 | 87 (62.6%) | 71 | 6.2 | 37.9% [28.5%, 48.4%] (n=87) | 36.5% [24.8%, 50.1%] (n=52) | 40.8% [30.2%, 52.5%] (n=71) | 21 (20.8%) | 33.3% |
| GD04-122 Jaburo | 2 | 70 (50.4%) | 59 | 6.5 | 35.7% [25.5%, 47.4%] (n=70) | 39.1% [28.5%, 50.9%] (n=69) | 39.0% [27.6%, 51.7%] (n=59) | 14 (16.3%) | 57.7% |
| ST01-001 Gundam | 4 | 106 (76.3%) | 92 | 7.1 | 41.5% [32.6%, 51.0%] (n=106) | 24.2% [12.8%, 41.0%] (n=33) | 46.7% [36.9%, 56.9%] (n=92) | 26 (14.7%) | 46.0% |
| ST01-002 Gundam (MA Form) | 2 | 79 (56.8%) | 63 | 7.1 | 39.2% [29.2%, 50.3%] (n=79) | 35.0% [24.2%, 47.6%] (n=60) | 44.4% [32.8%, 56.7%] (n=63) | 24 (26.1%) | 40.0% |
| ST01-004 Guntank | 3 | 90 (64.7%) | 77 | 6.0 | 35.6% [26.4%, 45.9%] (n=90) | 40.8% [28.2%, 54.8%] (n=49) | 36.4% [26.5%, 47.5%] (n=77) | 20 (15.4%) | 44.1% |
| ST01-005 GM | 3 | 101 (72.7%) | 93 | 5.0 | 39.6% [30.6%, 49.4%] (n=101) | 31.6% [19.1%, 47.5%] (n=38) | 40.9% [31.4%, 51.0%] (n=93) | 14 (10.0%) | 40.0% |
| ST01-010 Amuro Ray | 4 | 114 (82.0%) | 98 | 6.4 | 39.5% [31.0%, 48.6%] (n=114) | 28.0% [14.3%, 47.6%] (n=25) | 42.9% [33.5%, 52.7%] (n=98) | 35 (19.8%) | 48.9% |
| ST01-012 Thoroughly Damaged | 2 | 83 (59.7%) | 80 | 5.5 | 39.8% [29.9%, 50.5%] (n=83) | 33.9% [22.9%, 47.0%] (n=56) | 40.0% [30.0%, 51.0%] (n=80) | 7 (7.0%) | 51.5% |
| ST01-014 Unforeseen Incident | 3 | 89 (64.0%) | 89 | 5.5 | 38.2% [28.8%, 48.6%] (n=89) | 36.0% [24.1%, 49.9%] (n=50) | 38.2% [28.8%, 48.6%] (n=89) | 0 (0.0%) | 43.2% |
| ST04-013 Hawk of Endymion | 3 | 99 (71.2%) | 98 | 5.0 | 41.4% [32.2%, 51.3%] (n=99) | 27.5% [16.1%, 42.8%] (n=40) | 40.8% [31.6%, 50.7%] (n=98) | 1 (0.7%) | 50.0% |
| ST08-015 Davao | 2 | 78 (56.1%) | 68 | 5.8 | 39.7% [29.6%, 50.8%] (n=78) | 34.4% [23.7%, 47.0%] (n=61) | 41.2% [30.3%, 53.0%] (n=68) | 14 (14.9%) | 37.9% |

## Benchmark cards most associated with DUT losses

| Benchmark card | Seen games | DUT loss rate when seen | when not seen | Lift |
|---|---|---|---|---|
| GD01-030 Rick Dom | 103 | 67.0% [57.4%, 75.3%] (n=103) | 50.0% [34.5%, 65.5%] (n=36) | +17.0 pp |
| GD01-031 Gelgoog | 93 | 66.7% [56.6%, 75.4%] (n=93) | 54.3% [40.2%, 67.8%] (n=46) | +12.3 pp |
| GD04-040 Schuzrum-Galluss | 65 | 66.2% [54.0%, 76.5%] (n=65) | 59.5% [48.1%, 69.9%] (n=74) | +6.7 pp |
| GD01-044 Kshatriya | 69 | 65.2% [53.4%, 75.4%] (n=69) | 60.0% [48.3%, 70.7%] (n=70) | +5.2 pp |
| ST03-003 Geara Zulu | 64 | 64.1% [51.8%, 74.7%] (n=64) | 61.3% [50.0%, 71.5%] (n=75) | +2.7 pp |
| ST03-006 Char's Zaku Ⅱ | 98 | 63.3% [53.4%, 72.1%] (n=98) | 61.0% [45.7%, 74.3%] (n=41) | +2.3 pp |
| GD01-125 Zanzibar | 74 | 63.5% [52.1%, 73.6%] (n=74) | 61.5% [49.4%, 72.4%] (n=65) | +2.0 pp |
| ST03-008 Zaku Ⅱ | 102 | 62.7% [53.1%, 71.5%] (n=102) | 62.2% [46.1%, 75.9%] (n=37) | +0.6 pp |
| ST03-001 Sinanju | 63 | 61.9% [49.6%, 72.9%] (n=63) | 63.2% [51.9%, 73.1%] (n=76) | -1.3 pp |
| GD01-059 Zee Zulu | 93 | 60.2% [50.1%, 69.6%] (n=93) | 67.4% [53.0%, 79.1%] (n=46) | -7.2 pp |

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

- `replays/win-m0002-g3.json`: win, match 2 game 3, 20 turns, battle_damage
- `replays/win-m0035-g2.json`: win, match 35 game 2, 20 turns, battle_damage
- `replays/loss-m0009-g1.json`: loss, match 9 game 1, 18 turns, battle_damage
- `replays/loss-m0010-g2.json`: loss, match 10 game 2, 18 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
