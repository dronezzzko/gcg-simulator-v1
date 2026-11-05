# purple-white-tekkadan vs red-green-zeon

Deck under test (DUT) **purple-white-tekkadan** against benchmark **red-green-zeon**: 5 BO1 matches, master seed 8, AI preset `standard` (agents `test-random` / `test-random`, factory `random_agent.py:make_random_agent`).

Reproduce: `gcg-sim benchmark examples/decks/purple-white-tekkadan.txt examples/decks/red-green-zeon.txt --matches 5 --seed 8 --format bo1 --ai-preset standard --replays 0`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 5 | 0 | 5 | 0 | 0.0% [0.0%, 43.4%] (n=5) |
| Games | 5 | 0 | 5 | 0 | 0.0% [0.0%, 43.4%] (n=5) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 3 | 0 | 3 | 0 | 0.0% [0.0%, 56.2%] (n=3) |
| On the draw | 2 | 0 | 2 | 0 | 0.0% [0.0%, 65.8%] (n=2) |
| Game 1 | 5 | 0 | 5 | 0 | 0.0% [0.0%, 43.4%] (n=5) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 5 | 0 | 5 | 0 | 0.0% [0.0%, 43.4%] (n=5) |

Game length (turns): mean 18.8, median 18, p10 13, p90 28, range 13-28. Median by DUT result: win -, loss 18, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 60.0% [23.1%, 88.2%] (n=5); DUT win rate after a redraw 0.0% [0.0%, 56.2%] (n=3), after keeping 0.0% [0.0%, 65.8%] (n=2). Benchmark redraw rate 60.0% [23.1%, 88.2%] (n=5).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD02-054 Gundam Barbatos 1st Form | 4 | 2 (40.0%) | 2 | 7.0 | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 65.8%] (n=2) | 0 (0.0%) | n/a |
| GD02-058 Ryusei-Go (Graze Custom Ⅱ) | 2 | 4 (80.0%) | 3 | 4.7 | 0.0% [0.0%, 49.0%] (n=4) | 0.0% [0.0%, 79.3%] (n=1) | 0.0% [0.0%, 56.2%] (n=3) | 1 (25.0%) | 0.0% |
| GD02-114 It's Name is Ryusei-Go | 1 | 1 (20.0%) | 1 | 4.0 | 0.0% [0.0%, 79.3%] (n=1) | 0.0% [0.0%, 49.0%] (n=4) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
| GD03-056 Gundam Barbatos Adapt | 3 | 3 (60.0%) | 3 | 6.8 | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 56.2%] (n=3) | 0 (0.0%) | 100.0% |
| GD03-066 Gundam Barbatos 5th Form (Ground Type) | 2 | 3 (60.0%) | 1 | 5.5 | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 79.3%] (n=1) | 2 (50.0%) | 50.0% |
| GD05-058 Shiden Custom (Ryusei-Go) | 2 | 4 (80.0%) | 3 | 6.7 | 0.0% [0.0%, 49.0%] (n=4) | 0.0% [0.0%, 79.3%] (n=1) | 0.0% [0.0%, 56.2%] (n=3) | 1 (25.0%) | n/a |
| GD05-060 Gundam Flauros (Ryusei-Go) | 2 | 2 (40.0%) | 1 | 5.0 | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 79.3%] (n=1) | 3 (75.0%) | n/a |
| GD05-117 Become a Shield | 3 | 5 (100.0%) | 4 | 4.8 | 0.0% [0.0%, 43.4%] (n=5) | n/a (n=0) | 0.0% [0.0%, 49.0%] (n=4) | 1 (20.0%) | 66.7% |
| ST01-014 Unforeseen Incident | 3 | 3 (60.0%) | 3 | 6.0 | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 56.2%] (n=3) | 0 (0.0%) | 100.0% |
| ST05-001 Gundam Barbatos 4th Form | 3 | 5 (100.0%) | 4 | 7.4 | 0.0% [0.0%, 43.4%] (n=5) | n/a (n=0) | 0.0% [0.0%, 49.0%] (n=4) | 3 (37.5%) | 0.0% |
| ST05-002 Gundam Barbatos 2nd Form | 4 | 3 (60.0%) | 2 | 7.2 | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 65.8%] (n=2) | 1 (20.0%) | 100.0% |
| ST05-003 CGS Mobile Worker | 4 | 4 (80.0%) | 4 | 5.0 | 0.0% [0.0%, 49.0%] (n=4) | 0.0% [0.0%, 79.3%] (n=1) | 0.0% [0.0%, 49.0%] (n=4) | 1 (14.3%) | 66.7% |
| ST05-004 Graze Custom | 4 | 3 (60.0%) | 3 | 4.2 | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 56.2%] (n=3) | 0 (0.0%) | 33.3% |
| ST05-007 McGillis' Schwalbe Graze | 2 | 3 (60.0%) | 2 | 9.5 | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 65.8%] (n=2) | 1 (33.3%) | n/a |
| ST05-010 Mikazuki Augus | 4 | 4 (80.0%) | 4 | 6.7 | 0.0% [0.0%, 49.0%] (n=4) | 0.0% [0.0%, 79.3%] (n=1) | 0.0% [0.0%, 49.0%] (n=4) | 0 (0.0%) | 100.0% |
| ST05-011 Akihiro Altland | 3 | 4 (80.0%) | 3 | 5.0 | 0.0% [0.0%, 49.0%] (n=4) | 0.0% [0.0%, 79.3%] (n=1) | 0.0% [0.0%, 56.2%] (n=3) | 1 (25.0%) | 66.7% |
| ST05-012 McGillis Fareed | 2 | 3 (60.0%) | 3 | 8.7 | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 56.2%] (n=3) | 0 (0.0%) | n/a |
| ST05-015 Isaribi | 2 | 3 (60.0%) | 3 | 5.0 | 0.0% [0.0%, 56.2%] (n=3) | 0.0% [0.0%, 65.8%] (n=2) | 0.0% [0.0%, 56.2%] (n=3) | 1 (20.0%) | 50.0% |

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

- none

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
