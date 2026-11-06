# blue-white-federation vs purple-white-tekkadan

Deck under test (DUT) **blue-white-federation** against benchmark **purple-white-tekkadan**: 1 BO1 matches, master seed 5, AI preset `fast` (agents `test-random` / `test-random`, factory `random_agent.py:make_random_agent`).

Reproduce: `gcg-sim benchmark examples/decks/blue-white-federation.txt examples/decks/purple-white-tekkadan.txt --matches 1 --seed 5 --format bo1 --ai-preset fast --replays 1 --decision-log`

## Result

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| Matches | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |
| Games | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |

## Splits

|  | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| On the play | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |
| On the draw | 0 | 0 | 0 | 0 | n/a (n=0) |
| Game 1 | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |

## How games end

| Reason | n | DUT wins | Bench wins | Draws | DUT win rate [95% CI] |
|---|---|---|---|---|---|
| battle_damage | 1 | 0 | 1 | 0 | 0.0% [0.0%, 79.3%] (n=1) |

Game length (turns): mean 18.0, median 18, p10 18, p90 18, range 18-18. Median by DUT result: win -, loss 18, draw -.

Draws: 0 (0 simultaneous defeat, 0 turn limit); 0 scored for BO3 by the turn-player-loses rule (TRM 5.2).

## Redraws

DUT redraw rate 0.0% [0.0%, 79.3%] (n=1); DUT win rate after a redraw n/a (n=0), after keeping 0.0% [0.0%, 79.3%] (n=1). Benchmark redraw rate 100.0% [20.7%, 100.0%] (n=1).

## Deck-under-test cards

| Card | Copies | Drawn games | Played games | Mean own turn played | Win rate when drawn | Win rate when not drawn | Win rate when played | Drawn, never played | Redrawn when in initial hand |
|---|---|---|---|---|---|---|---|---|---|
| GD01-001 Gundam | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| GD01-004 Guncannon | 4 | 1 (100.0%) | 0 | - | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | n/a (n=0) | 1 (100.0%) | n/a |
| GD01-008 Guntank | 4 | 1 (100.0%) | 1 | 2.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | 0.0% |
| GD01-087 Sayla Mass | 2 | 1 (100.0%) | 1 | 3.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 1 (50.0%) | n/a |
| GD01-099 Intercept Orders | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| GD03-001 Gundam NT-1 | 2 | 1 (100.0%) | 0 | - | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | n/a (n=0) | 1 (100.0%) | n/a |
| GD04-008 Gundam | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| GD04-012 Core Booster (005) & Core Booster (006) | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| GD04-084 Sleggar Law | 2 | 1 (100.0%) | 1 | 3.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 1 (50.0%) | 0.0% |
| GD04-122 Jaburo | 2 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-001 Gundam | 4 | 1 (100.0%) | 1 | 8.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 1 (50.0%) | 0.0% |
| ST01-002 Gundam (MA Form) | 2 | 1 (100.0%) | 0 | - | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | n/a (n=0) | 1 (100.0%) | n/a |
| ST01-004 Guntank | 3 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-005 GM | 3 | 1 (100.0%) | 1 | 5.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
| ST01-010 Amuro Ray | 4 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| ST01-012 Thoroughly Damaged | 2 | 1 (100.0%) | 1 | 9.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | n/a |
| ST01-014 Unforeseen Incident | 3 | 1 (100.0%) | 0 | - | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | n/a (n=0) | 1 (100.0%) | n/a |
| ST04-013 Hawk of Endymion | 3 | 0 (0.0%) | 0 | - | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0 (n/a) | n/a |
| ST08-015 Davao | 2 | 1 (100.0%) | 1 | 3.0 | 0.0% [0.0%, 79.3%] (n=1) | n/a (n=0) | 0.0% [0.0%, 79.3%] (n=1) | 0 (0.0%) | 0.0% |

## Benchmark cards most associated with DUT losses

No benchmark card has 20+ games both seen and unseen yet.

## Tuning hypotheses (to test, not conclusions)

No signal cleared the noise gate (a two-proportion z-test significant at 0.05 after a Holm correction per kind of comparison, with at least 20 games per side (10 for redraws)). Run more games.

## Versions

gcg-sim 0.1.0; results schema 1.1.0; card data 31-ebd08a4b4f8c8898bc407498229839548fb911e9 (built 2026-11-02T14:57:40.351Z); Comprehensive Rules 1.10.0 (2026-10-16); Banned & Restricted list 2026-09-25; BO3 rules 2026-07-03.

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

- `replays/loss-m0000-g1.json`: loss, match 0 game 1, 18 turns, battle_damage

## Notes

- Win rates count draws as non-wins; n is every game (or match). Intervals are 95% Wilson score intervals.
- Both seats are played by the same AI (see 'ai'); results measure the decks under that AI, not under human play.
- Per-card and benchmark-card signals are correlations within these games; the 'hypotheses' list suggests what to test next and is not a conclusion.
- A simultaneous defeat (both_defeated) is a game draw; in BO3 match scoring the turn player at the end loses it (TRM 5.2). turn_limit games are engine safety-cap draws and are always counted and reported.
