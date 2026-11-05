# Inspection notes: surprising results of the final evaluation (T-144)

The brief asks that replays of surprising results be saved and inspected. These notes cover
final evaluation 2 (`docs/dev/eval/final-2/`, shipped M15b presets, eval masters
90300001-90300007); final evaluation 1 is in `docs/dev/eval/final-1/README.md`.

## The 14 games `standard` lost to random (E3, panel)

`search:standard` won 11186 of 11200 panel games against `random`
(`final-2/e3-panel-standard-vs-random.json`, master 90300004). Every loss is saved as a verified
replay: `gcg-eval head-to-head-replay final-2/e3-panel-standard-vs-random.json --losses --out
final-2/e3-losses` re-played the 14 games from the report's own specs, each matching its
outcome row, and `gcg-sim replay` verifies all 14. `final-2/e3-losses/losses.json` lists them
for `gcg-eval gates --losses-documented`, which checks each replay with `check_replay` against
its row.

| Game | `standard`'s deck | random's deck | `standard`'s seat | turns | units deployed, `standard` / random | attacks on the player, `standard` / random |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 1395 | blue-white-federation | blue-red-tempo | 1 | 21 | 8 / 14 | 4 / 10 |
| 2516 | blue-white-federation | red-purple-control | 0 | 26 | 12 / 8 | 3 / 6 |
| 2961 | red-green-zeon | purple-white-tekkadan | 1 | 19 | 7 / 10 | 6 / 8 |
| 5877 | blue-red-tempo | red-green-zeon | 1 | 21 | 8 / 13 | 4 / 8 |
| 6113 | green-white-link | red-green-zeon | 1 | 21 | 7 / 16 | 5 / 7 |
| 6185 | green-white-link | red-green-zeon | 1 | 25 | 7 / 17 | 4 / 10 |
| 7027 | red-green-zeon | red-purple-control | 1 | 25 | 7 / 11 | 4 / 7 |
| 8597 | red-purple-control | blue-green-ramp | 1 | 17 | 6 / 12 | 6 / 6 |
| 10275 | green-white-link | purple-blue-swarm | 1 | 23 | 9 / 15 | 7 / 9 |
| 10363 | green-white-link | purple-blue-swarm | 1 | 21 | 6 / 13 | 2 / 10 |
| 10475 | green-white-link | red-purple-control | 1 | 25 | 9 / 10 | 9 / 7 |
| 10535 | green-white-link | red-purple-control | 1 | 29 | 10 / 13 | 10 / 7 |
| 10933 | red-purple-control | purple-blue-swarm | 1 | 29 | 8 / 16 | 4 / 11 |
| 10949 | red-purple-control | purple-blue-swarm | 1 | 19 | 8 / 13 | 3 / 12 |

Counts are from each replay's log. Every game ended by battle damage.

What the replays show:

- `standard` was on the draw (seat 1; `--choose-first seat0` puts seat 0 on the play) in 13 of
  the 14 games, so it was a turn behind every time.
- The random agent deployed more units than `standard` in 13 of the 14 games. Choosing
  uniformly among the legal actions, it plays a card on most main-phase decisions, so with a
  deck that curves out it floods the board.
- `green-white-link`, a held-out deck, was `standard`'s deck in 6 of the 14 losses. In game
  10363 it deployed nothing on its first two turns (turns 2 and 4, `end_main` with resources
  open) and its first unit on turn 6, while random's swarm deck kept deploying; random won on
  turn 21. This is the early passivity that the blinded audit
  (W-17, `final-2/audit/README.md`) found most often.
- Game 8597: `standard` (`red-purple-control`, on the draw) removed units with Darkness Finger
  and attacked, but random's `blue-green-ramp` deployed Nu Gundam, Penelope and a stream of
  Re-GZ units; on turn 16 `standard` deployed Sazabi and did not attack, and on turn 17
  random attacked the player four times and won.

No loss shows an engine or rules fault: each replay re-simulates to its recorded final state.
They are strategic losses to a random opponent whose deck developed faster, concentrated in
games `standard` played on the draw.

## Below the bar: E4 on the AI test decks

`search:standard` against `greedy` on the AI test decks won 304/400 (0.760, [0.716, 0.799];
`final-2/e4-tests-standard-vs-greedy.json`, master 90300005), below the brief's bar of a lower
bound above 0.764 (the upper end of the reference's own interval, 289/400 [0.677, 0.764]).
Final evaluation 1 gave 307/400. On the panel the same agents give 8774/11200 (0.783, [0.776,
0.791]), and 0.779 [0.767, 0.790] on the held-out pairs alone, which clear the bar. So on the
two test decks the strengthened `standard` beats `greedy` about 4 points more often than the
reference does (304 against 289 of 400), which is not enough for a lower bound above 0.764 at
400 games (that needs about 323/400).

## E1 held-out pairs alone

`search:standard` against `ref:strong` won 6408/11200 (0.572, [0.563, 0.581]) over all pairs and
2920/5200 (0.562, [0.548, 0.575]) over the 13 held-out pairs. The brief's bar (pooled lower bound
at least 0.55, no held-out pair with an upper bound below 0.5) holds. The plan's stricter A26
reading, which also asks for a lower bound of at least 0.55 over the held-out pairs alone,
misses by 0.002. The weakest pair is `blue-white-federation` against `green-white-link`
(0.530).

## `fast`'s fidelity on one weakened variant (W-18)

On the shipped presets and eval master 90300007, `fast` ranks the five deck variants like
`standard` (Spearman 0.9, the same best variant), but on `blue-white-federation-weakened` it
wins 59/200 against `standard`'s 74/200 on the same deals (-0.075 [-0.146, -0.003];
`final-2/fidelity-fast-standard.json`). The screening workflow therefore confirms every
survivor with `standard`.
