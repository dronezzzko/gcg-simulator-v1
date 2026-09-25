# AI players

`gcg_sim.ai` provides the players that the benchmark runner pits against each other:

| Kind | What it does | Cost |
| --- | --- | --- |
| `random` | Uniformly random legal action from a seeded generator. | ~0 |
| `greedy` | One-ply look-ahead on one determinization, scored by the evaluation. | ~0.01 s per decision |
| `mcts` | Determinized information-set MCTS with rollouts through the opponent's reply turn. | see [Presets](#presets-and-measured-throughput) |

```python
from gcg_sim.ai import make_agent
from gcg_sim.ai.tuning import TUNING_DECKS
from gcg_sim.engine.game import apply, new_game

state = new_game((TUNING_DECKS[0], TUNING_DECKS[1]), 7)
agent = make_agent("mcts", preset="standard", seed=1234, log=True)
action = agent.choose(state, state.pending.player)  # one of state.pending.options
apply(state, action)
print(agent.decision_log())  # [{"turn", "player", "decision", "chosen", "label", "alternatives"}]
```

Every agent is a deterministic function of its construction seed and the sequence of
`choose` calls: decision number *n* draws its randomness from
`SplitMix64(derive_seed(seed, agent_name, "decision", n))`. Nothing reads the wall clock or a
global random generator.

## Information-set guarantees

A player sees the decklists (open information), public zones, its own hand, the number of
cards in hidden zones, and cards it has looked at. The agents respect that boundary:

- From the true state an agent reads only `st.pending` (its own legal options) and what it
  may know; every look-ahead runs on `gcg_sim.engine.observe.determinize(st, player, seed)`,
  which redraws every card hidden from the player from the open decklists and replaces the
  game RNG (future shuffles and draws).
- Search seeds come from the agent seed and the decision counter, never from the state.
- Move keys (below) identify hand, trash, and deck cards by definition only when the
  deciding player knows them; otherwise by instance number, which carries no identity
  (instance numbers are assigned after the shuffle).
- `tests/ai/test_information_set.py` takes 25 mid-game states from greedy self-play, shuffles
  the hidden identities with `permute_hidden` (three permutations each: a different true state
  inside the same information set), and checks that a fresh MCTS agent and a fresh greedy
  agent with the same seed return the same move **and** the same search statistics. As a
  negative control, a search that uses the true state instead of a determinization changes
  its decision on 25 of the 25 states.

## Architecture of the MCTS player (`mcts.py`)

Single-observer information-set MCTS (SO-ISMCTS) over *moves*:

1. **Moves.** `actions.canonical` merges legal actions that are the same move: two copies of a
   card in hand, or the same play paid with a different number of EX Resources (the
   representative spends the fewest EX Resources, which are single-use). A move key is
   `(kind, card, target, play-variant)` with hand/trash/deck cards named by definition.
2. **Root.** Each iteration picks a root move by UCB1 with a progressive prior:
   `mean + c·sqrt(ln(avail)/visits) + w·prior/(visits+1)` (`c = exploration`, 0.3 in the
   standard preset; `w = prior_weight = 0.5`). Moves not yet tried go first, highest prior
   first. Priors are the playout policy's scores through a softmax (temperature 1.5).
3. **Common random numbers.** The *n*-th visit of every root move is played in the *n*-th
   determinization with the *n*-th rollout seed, so root moves are compared on identical
   worlds (paired sampling). This alone raised the win rate against greedy from 0.61 to 0.68
   in 192-game mirrored tests.
4. **Tree.** Below the root the walk chooses among the moves legal in the current world;
   children are keyed by `(mover, move key)` and keep availability counts, so a move that
   exists only in some worlds (the opponent holding a certain card) is judged fairly.
   Decisions with one legal move are applied without a node. One node is added per
   iteration.
5. **Rollout.** The heuristic playout policy (softmax over its scores, temperature 0.5 in the
   standard preset) plays both sides until `horizon_turns = 2` turn boundaries have passed:
   the rest of the current turn and the whole reply turn, stopping at the start of the turn
   after that.
6. **Leaf value** (for the searching player): a finished game scores `1 - 0.01·turns elapsed`
   for a win and `0.01·turns elapsed` for a loss (win sooner, lose later; 0.5 for a draw);
   otherwise `0.05 + 0.9·σ(score / value_scale)` with `value_scale = 2`, so a certain result
   always outranks an estimate and lopsided positions keep a usable gradient.
7. **Budget and choice.** A decision with *n* distinct moves gets
   `clamp(iterations_per_option·n, min_iterations, max_iterations)` iterations; the search stops
   early once the most-visited move cannot be overtaken. The move with the most visits is
   played (ties: higher mean, then option order).

Pre-game choices (`setup.py`):

- **Play/draw** (`CHOOSE_FIRST`): policy-vs-policy games from both choices on the same
  determinizations (`first_player_games` pairs); the agent goes second only when that is better
  by more than two standard errors of the paired difference. In MCTS self-play on the tuning
  decks Player One won 436 of 720 games (60.6%), and the agent goes first with the test decks.
- **Mulligan** (`REDRAW`): `opening_score` plays the hand out greedily over the first four
  turns (resources = turn number, plus Player Two's EX Resource) and weights early board
  development most. The agent redraws when the hand scores below 90% of the average of
  `mulligan_samples` fresh five-card hands drawn from the rest of the deck (the returned hand
  goes to the bottom, rule 6-2-1-6-1, so it cannot come back).

## Playout policy (`policy.py`)

The policy scores every option of every decision kind from the current board, without
look-ahead; search uses it for rollouts and priors, and greedy uses it (deterministically) to
finish the turn.

| Decision | Heuristic |
| --- | --- |
| Main phase | Play the largest affordable Unit; pair Pilots, strongly preferring one that links (more so on a Unit deployed this turn, which can then attack); replace an EX Base; play Commands and abilities; spend EX Resources only when needed. |
| Attacks | Lethal on an exposed player first; with more Blockers than attackers, play removal first. Attack a rested Unit when it dies and the attacker survives; avoid attacks into a Blocker that wins the fight; small penalty when the attacker will be exposed to a stronger enemy Unit on the reply turn. |
| Blocks | Must-block when the player would lose; otherwise value the Shield or Unit saved plus the fight outcome. |
| Action steps | Tricks during a battle are tried; passing is preferred in the end phase. |
| Targets (`SELECT`) | Reads the resolving effect's program to classify the choice as helping or harming the chosen cards, then picks the most valuable enemy card to harm or friendly card to help (least valuable for costs); rest-substitution picks cheaper cards. |
| `EXCESS` / `DISCARD` | Trash or discard the least valuable card. |
| `ARRANGE` | Keep valuable cards on top of your own deck. |
| `BURST` / `YES_NO` | Yes. |
| `CHOOSE_FIRST` / `REDRAW` | Go first / keep (search agents use the pre-game procedures above). |

## Evaluation (`evaluation.py`)

A logistic model of the probability that `player` wins. Each feature is
`side(player) − side(opponent)` except `tempo`; the vector is antisymmetric, so the two players'
estimates add up to 1. The search evaluates determinized states only, so the opponent's hand
composition is a sample from the player's information set.

| Feature | Meaning (per side) | Hand-set | Tuned |
| --- | --- | ---: | ---: |
| `tempo` | +1 if `player` is the turn player, −1 otherwise | 0.00 | −0.20 |
| `life` | Shields + Base HP / 3 | 0.35 | 0.43 |
| `danger` | max(0, 3 − life) | −0.30 | −0.44 |
| `can_kill` | next attack step has enough attackers (minus Blockers not answered by castable removal in hand) to break every Shield and the Base and hit the player | 1.00 | 0.53 |
| `ap` | total AP of Units | 0.10 | 0.11 |
| `hp` | total remaining HP of Units | 0.10 | 0.16 |
| `linked` | Link Units | 0.20 | 0.21 |
| `ready` | active (unrested) Units | 0.20 | 0.53 |
| `hand_units` | Unit cards in hand | 0.30 | 0.72 |
| `hand_pilots` | Pilot cards in hand | 0.25 | 0.49 |
| `hand_commands` | Command cards in hand | 0.25 | 0.37 |
| `hand_bases` | Base cards in hand | 0.25 | 0.57 |
| `level` | cards in the resource area | 0.15 | 0.16 |
| `bursts` | Shields × fraction of 【Burst】 cards among the cards this player cannot see (from the open decklist) | 0.30 | 0.55 |

The negative `tempo` weight reflects when positions are recorded: at the turn player's first
decision, after its draw and resource placement, which the hand and level features already
count.

### Feature selection

Candidate features were compared by five-fold cross-validated log-loss (folds by game) on
11,792 turn-start positions from 720 MCTS self-play games. Splitting the hand by card type
lowered log-loss from 0.515 to 0.491; features that did not help (Unit count, Blocker count,
EX Resources, deck size, exposed flag, maximum AP, trash size, a continuous attack margin)
were dropped.

### Tuning procedure (`tuning.py`)

1. Self-play: two MCTS agents with the small `tuning` budget (16–48 iterations) and the
   current weights play `--games` games on the three tuning decks (Zeon green/red, Wing
   green/blue, Tekkadan purple), cycling through all nine ordered deck pairings. The tuning
   decks are different from the AI test decks.
2. At the turn player's first main-phase decision of every turn, record its feature vector;
   label it 1 if that player won the game (draws dropped).
3. Fit an L2-regularised (λ = 1) logistic regression by Newton's method on the even-indexed
   games of all rounds so far; report log-loss on the current round's odd-indexed games
   before (current weights) and after (fitted weights).
4. Repeat with the fitted weights; the final weights are fitted on every position of every
   round.
5. Validate: a mirrored head-to-head match between the tuned and hand-set weights.

Every seed derives from `--seed` (default 2026); the result does not depend on `--workers`.
The run that produced `TUNED_WEIGHTS` was
`python -m gcg_sim.ai.tuning --games 540 --rounds 2 --workers 13 --validate 216 --validate-preset standard`
(13 min on 13 cores):

| Round | Positions | Held-out log-loss before | after |
| --- | ---: | ---: | ---: |
| 0 (hand-set weights) | 8,605 | 0.5452 | 0.5187 |
| 1 (round-0 weights) | 7,957 | 0.5218 | 0.5188 |

Final fit on 16,562 positions. Validation, standard preset, 216 mirrored games: tuned 112,
hand-set 104 (51.9%, Wilson 95% [0.452, 0.584]) — the tuned model predicts outcomes better,
and in play it is at least as strong as the hand-set one. An earlier round with the first
feature set (whole-hand count) showed a clearer effect: tuned against hand-set weights won
128 of 192 games (66.7%, [0.597, 0.730]).

A small version of the procedure runs in the test suite
(`tests/ai/test_tuning.py`: three games, one round, run twice and compared bit for bit), and
from the command line:

```bash
uv run python -m gcg_sim.ai.tuning --games 2 --rounds 1
```

## Presets and measured throughput

| Preset | Iterations per decision | Other |
| --- | --- | --- |
| `standard` (default) | clamp(32·n, 80, 320) | exploration 0.3, rollout temperature 0.5, 24 play/draw game pairs, 96 mulligan samples |
| `strong` | clamp(128·n, 320, 1280) | exploration 0.15, rollout temperature 0.3, 48 play/draw game pairs, 192 mulligan samples |

Measured on the integrated engine (commit 013ea8f, 8 worker processes on an otherwise idle
14-core Apple-silicon machine; core-seconds = wall time × workers):

| Measurement | Games | Mean turns | Wall time | Core-seconds per game | Games per core-minute |
| --- | ---: | ---: | ---: | ---: | ---: |
| `gcg_sim.ai.throughput --preset standard` (tuning decks, MCTS vs MCTS) | 24 | 14.3 | 38.0 s | 12.7 | 4.74 |
| `gcg_sim.ai.throughput --preset strong` (tuning decks, MCTS vs MCTS) | 16 | 15.0 | 121.8 s | 60.9 | 0.99 |
| `gcg-sim benchmark` red-green-zeon vs blue-white-federation, 16 BO3 matches, standard | 44 | 15.7 | 100.2 s | 18.2 | 3.29 |
| `gcg-sim benchmark` red-green-zeon mirror, 32 BO3 matches, standard | 81 | — | 107.9 s | 10.7 | 5.63 |

So 100 BO3 matches of the example decks (about 250–275 games) take about 6–11 minutes on 8
cores with `standard`, well inside the 30-minute target; `strong` is about 4–5× slower.
During development, on the AI branch's older engine and the AI test decks (Federation vs
SEED, 13 workers), `standard` measured 1.65 games per core-minute and `strong` 0.44.
About 90% of search time is spent inside the engine's `apply`, two thirds of it recomputing
derived characteristics (`engine/view.py:_compute`); the playout policy and evaluation take
under 10%.

Measure with (wall-clock is read only to report speed):

```bash
uv run python -m gcg_sim.ai.throughput --preset standard --games 1
```

## Strength

Acceptance test `tests/slow/test_ai_strength.py` (default `standard` preset, test decks
Federation blue/white and SEED red/white from `tests/ai/decks.py`, 400 games per opponent in
mirrored blocks of four so each deal is played with the MCTS agent on both decks and in both
seats). Run on the integrated engine (commit 013ea8f, 12 worker processes, 10.1 min for all
800 games):

| Opponent | MCTS score | Win rate | Wilson 95% interval | Mean turns |
| --- | ---: | ---: | --- | ---: |
| random | 400 / 400 | 1.000 | [0.990, 1.000] | 11.4 |
| greedy | 276 / 400 | 0.690 | [0.643, 0.733] | 17.6 |

(On the AI branch's older engine the same test scored 399/400 and 272/400.)

Greedy is a demanding baseline: it shares the tuned evaluation and the playout policy, and
it compares its moves on a single world (so its comparisons have no sampling noise).

`strong` against `standard` (measured during development on the older engine; test decks,
mirrored): 58 of 96 games (seed 7) and 103 of 192
(seed 8), together 161 of 288 = 0.559, Wilson 95% [0.501, 0.615]. Simply giving the standard
knobs 4× the iterations did not help (46 of 96, 0.479), nor did a three-turn horizon at 2×
(51 of 96); what helps a large budget is spending it on the best lines (exploration 0.15) with
less noisy rollouts (temperature 0.3). Those two settings at the standard budget were not
better than the standard ones (91 of 192, 0.474).

Tactical puzzles (`tests/ai/test_puzzles.py`, each solved by the standard preset with two
agent seeds against an MCTS defender): lethal on an exposed player; lethal by going face
instead of taking a free kill; rest the Blocker with removal before the lethal attack; pair
the linking Pilot so a Unit deployed this turn can attack for lethal; choose the Pilot and
Unit that link; order attacks around a known 【Burst】 (attack with the Unit the Burst could
rest first); hold removal for next turn's lethal instead of wasting it (the agent even casts
it in the opponent's end step so the Blocker stays rested); take a favourable trade; do not
attack into a bigger Blocker; block to survive; block when the Blocker wins the fight; use an
action-step trick (AP−3) to survive; aim removal at the most valuable Unit it can destroy;
trash the weakest Unit to the battle-area limit; take a free card from 【Burst】; mulligan a
hand of uncastable cards and keep a hand with early plays; go first.

## Known weaknesses

- **Horizon.** Rollouts stop at the start of the turn after the reply turn; plans that pay
  off later rely on the evaluation (for example `can_kill` counts castable removal in hand).
- **Policy bias.** Rollouts and priors come from a hand-written policy: it plays Commands
  somewhat eagerly, judges modal choices and activated abilities coarsely, and models the
  opponent as playing the same way.
- **Strategy fusion.** Determinized search can assume it will "know" hidden cards when a
  later decision is made; common random numbers at the root reduce, but do not remove, this.
- **Linear evaluation.** The model has no interaction terms (for example between life and
  the opponent's board) and is fitted to self-play of the tuning budget.
- **Budget scaling.** Against greedy the win rate grows slowly with iterations: before paired
  sampling, 4× the iterations moved it from 0.70 to 0.77, and with paired sampling 2.5× left
  it at 0.69–0.71 (192 games each). The standard preset therefore spends a moderate budget;
  the strong preset is for confirmation runs.
- **Engine-bound speed.** In a profiled standard game, 91% of the time was inside the
  engine's `apply` and 69% in `engine/view.py:_compute` (338,330 recomputations for 92,691
  applied actions; 20.8 million `_host_abilities` calls, mostly for cards in hand and trash).
  Caching per-definition ability lists by zone in the engine would speed search up directly.
