# blue-white-federation vs red-green-zeon

Deck under test (DUT) **blue-white-federation** against benchmark **red-green-zeon**: 160 BO1 matches, master seed 3, AI preset `standard` (agents `fake` / `fake`, factory `random_agent.py:make_random_agent`).

Reproduce: `gcg-sim benchmark examples/decks/blue-white-federation.txt examples/decks/red-green-zeon.txt --matches 160 --seed 3 --format bo1 --ai-preset standard --replays 2`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 160 | 99 | 57 | 4 | 61.9% [54.2%, 69.0%] (n=160) |
| Games | 160 | 99 | 57 | 4 | 61.9% [54.2%, 69.0%] (n=160) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 106 | 69 | 35 | 2 | 65.1% [55.6%, 73.5%] (n=106) |
| On the draw | 54 | 30 | 22 | 2 | 55.6% [42.4%, 68.0%] (n=54) |
| Game 1 | 160 | 99 | 57 | 4 | 61.9% [54.2%, 69.0%] (n=160) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 160 | 99 | 57 | 4 | 61.9% [54.2%, 69.0%] (n=160) |

Game length (turns): mean 13.9, median 14, p10 9, p90 19, range 8-20. Median by DUT result: win 13, loss 14, draw 16.

Draws: 4 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 25.0% [18.9%, 32.2%] (n=160); DUT win rate after a redraw 72.5% [57.2%, 83.9%] (n=40), after keeping 58.3% [49.4%, 66.8%] (n=120). Benchmark redraw rate 0.0% [0.0%, 2.3%] (n=160).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-001 Gundam | 2 | 80 (50.0%) | 80 | 2.0 | 75.0% [64.5%, 83.2%] (n=80) | 48.8% [38.1%, 59.5%] (n=80) | 75.0% [64.5%, 83.2%] (n=80) | 0 (0.0%) | 50.0% |
| GD01-004 Guncannon | 4 | 80 (50.0%) | 80 | 2.0 | 48.8% [38.1%, 59.5%] (n=80) | 75.0% [64.5%, 83.2%] (n=80) | 48.8% [38.1%, 59.5%] (n=80) | 0 (0.0%) | 0.0% |
| GD01-008 Guntank | 4 | 80 (50.0%) | 0 | - | 75.0% [64.5%, 83.2%] (n=80) | 48.8% [38.1%, 59.5%] (n=80) | n/a (n=0) | 80 (100.0%) | 50.0% |
| GD01-087 Sayla Mass | 2 | 80 (50.0%) | 0 | - | 48.8% [38.1%, 59.5%] (n=80) | 75.0% [64.5%, 83.2%] (n=80) | n/a (n=0) | 80 (100.0%) | 0.0% |
| GD01-099 Intercept Orders | 2 | 80 (50.0%) | 80 | 2.0 | 75.0% [64.5%, 83.2%] (n=80) | 48.8% [38.1%, 59.5%] (n=80) | 75.0% [64.5%, 83.2%] (n=80) | 0 (0.0%) | 50.0% |
| GD03-001 Gundam NT-1 | 2 | 80 (50.0%) | 80 | 2.0 | 48.8% [38.1%, 59.5%] (n=80) | 75.0% [64.5%, 83.2%] (n=80) | 48.8% [38.1%, 59.5%] (n=80) | 0 (0.0%) | 0.0% |
| GD04-008 Gundam | 2 | 80 (50.0%) | 0 | - | 75.0% [64.5%, 83.2%] (n=80) | 48.8% [38.1%, 59.5%] (n=80) | n/a (n=0) | 80 (100.0%) | 50.0% |
| GD04-012 Core Booster (005) & Core Booster (006) | 2 | 80 (50.0%) | 0 | - | 48.8% [38.1%, 59.5%] (n=80) | 75.0% [64.5%, 83.2%] (n=80) | n/a (n=0) | 80 (100.0%) | 0.0% |
| GD04-084 Sleggar Law | 2 | 80 (50.0%) | 80 | 2.0 | 75.0% [64.5%, 83.2%] (n=80) | 48.8% [38.1%, 59.5%] (n=80) | 75.0% [64.5%, 83.2%] (n=80) | 0 (0.0%) | 50.0% |
| GD04-122 Jaburo | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-001 Gundam | 4 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-002 Gundam (MA Form) | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-004 Guntank | 3 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-005 GM | 3 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-010 Amuro Ray | 4 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-012 Thoroughly Damaged | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-014 Unforeseen Incident | 3 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |
| ST04-013 Hawk of Endymion | 3 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |
| ST08-015 Davao | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 61.9% [54.2%, 69.0%] (n=160) | n/a (n=0) | 0 (n/a) | n/a |

## Benchmark cards most associated with DUT losses

| Benchmark card | Seen games | DUT loss rate when seen | when not seen | Lift |
|---|---|---|---|---|
| GD01-030 Rick Dom | 80 | 22.5% [14.7%, 32.8%] (n=80) | 48.8% [38.1%, 59.5%] (n=80) | -26.2 pp |

## Tuning hypotheses (to test, not conclusions)

Comparison signals (win-rate differences) cleared the noise gate: a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws). 'Never played' signals are descriptive.

- Games where Gundam (GD01-001) was drawn were won 75% of the time vs 49% when it was not (n=80/80). Hypothesis: more copies or more ways to find it may raise the win rate. Correlation only: longer games also draw more cards. (sample: 160; p = 0.00063, 9 comparison(s) of this kind)
- Games where Guntank (GD01-008) was drawn were won 75% of the time vs 49% when it was not (n=80/80). Hypothesis: more copies or more ways to find it may raise the win rate. Correlation only: longer games also draw more cards. (sample: 160; p = 0.00063, 9 comparison(s) of this kind)
- Games where Intercept Orders (GD01-099) was drawn were won 75% of the time vs 49% when it was not (n=80/80). Hypothesis: more copies or more ways to find it may raise the win rate. Correlation only: longer games also draw more cards. (sample: 160; p = 0.00063, 9 comparison(s) of this kind)
- Games where Guncannon (GD01-004) was drawn were won 49% of the time vs 75% when it was not (n=80/80). Hypothesis: it may underperform in this matchup; test cutting a copy. Correlation only: longer games also draw more cards. (sample: 160; p = 0.00063, 9 comparison(s) of this kind)
- Games where Sayla Mass (GD01-087) was drawn were won 49% of the time vs 75% when it was not (n=80/80). Hypothesis: it may underperform in this matchup; test cutting a copy. Correlation only: longer games also draw more cards. (sample: 160; p = 0.00063, 9 comparison(s) of this kind)
- Games where Gundam NT-1 (GD03-001) was drawn were won 49% of the time vs 75% when it was not (n=80/80). Hypothesis: it may underperform in this matchup; test cutting a copy. Correlation only: longer games also draw more cards. (sample: 160; p = 0.00063, 9 comparison(s) of this kind)
- 100% of drawn copies of Guntank (GD01-008) were never played (n=80 copies; 0 still in hand at game end). Hypothesis: it is often uncastable or low priority in this matchup; consider fewer copies or a lower curve. (sample: 80)
- 100% of drawn copies of Sayla Mass (GD01-087) were never played (n=80 copies; 0 still in hand at game end). Hypothesis: it is often uncastable or low priority in this matchup; consider fewer copies or a lower curve. (sample: 80)
- 100% of drawn copies of Gundam (GD04-008) were never played (n=80 copies; 0 still in hand at game end). Hypothesis: it is often uncastable or low priority in this matchup; consider fewer copies or a lower curve. (sample: 80)
- 100% of drawn copies of Core Booster (005) & Core Booster (006) (GD04-012) were never played (n=80 copies; 0 still in hand at game end). Hypothesis: it is often uncastable or low priority in this matchup; consider fewer copies or a lower curve. (sample: 80)

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

- `replays/win-m0005-g1.json`: win, match 5 game 1, 13 turns, battle_damage
- `replays/win-m0018-g1.json`: win, match 18 game 1, 13 turns, battle_damage
- `replays/loss-m0006-g1.json`: loss, match 6 game 1, 14 turns, battle_damage
- `replays/loss-m0019-g1.json`: loss, match 19 game 1, 14 turns, battle_damage
- `replays/draw-m0034-g1.json`: draw, match 34 game 1, 16 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
