# P6-H weakness status: pre-game choices (S11; W-44, W-45)

Package P6-H (docs/dev/PLAN.md §12.2, §12.3 S11): the flags `exclude_capped_play_draw` (W-44)
and `paired_mulligan` (W-45). Written by the P6-H implementer on 2026-11-02 in worktree branch
`worktree-wf_b2d4a964-3bd-4`, from `8ae4767` ("Port M11"). WEAKNESSES.md is not edited here (the
lead records the status lines from this file, §12.10). Both flags stay off in the shipped presets
(`presets.rs` is unchanged); adopting them is the lead's decision (§12.8, §12.9 item 4).

Commits: 86e481d (the mechanisms and their unit tests), e6a6204 (the scenario tests), and the
commit that adds this file and `docs/dev/eval/P6-H/`. Every ablation row ran the release
`gcg-eval` built from the mechanism code of the P6-H worktree at 86e481d (integrated at
M13 and squashed into commit 83a06c8); the exact commands are in
`docs/dev/eval/P6-H/README.md`.

## Shared machinery: paired pre-game simulations

- `crates/gcg-search/src/mech/p6h/sim.rs`: `simulate(cx, choices, seeds, rollout)` plays one line
  per `(choice, seed)`: the world `determinize(info, player, seed)` built from the observer's
  information-set state (the `PregameCx` state, W-135), `apply(choice, check=True)`, then
  `Mechanisms::rollout_choice` of the configuration for both players with `SplitMix64(seed)`
  until the game ends, an optional stop turn is reached, or the decision cap is hit, and the
  final state is scored by `Mechanisms::leaf_value` (an engine error inside the rollout is
  scored by `Mechanisms::error_leaf` when `engine_error_leaf` is on, and fails the decision
  otherwise, as in the reference). Every choice is played on the same seeds (common random
  numbers). The lines run as one `fork_join` on the work pool with the results in input order,
  every seed is drawn from the per-decision generator in a fixed order, and nothing reads a
  clock or the thread count. Because the lines go through `Composed`, a rollout policy or an
  evaluation that P6-F or P6-G add later is used here as soon as its flag is on (W-44's "uses
  its calibrated rollout policy (W-23)"); with those flags off the lines play the reference
  policy and evaluation, so each individual row below measures its own mechanism alone.
- `crates/gcg-search/src/mech/p6h/mod.rs`: `PROVIDES` and `pregame(flags, cx)`: REDRAW with
  `paired_mulligan`, CHOOSE_FIRST with `exclude_capped_play_draw`, otherwise
  `reference::pregame`.
- `crates/gcg-search/src/mech/mod.rs` (the p6h wiring lines only): `mod p6h`,
  `Composed::pregame` dispatching to `p6h::pregame`, and `p6h::PROVIDES` in `available`.
- Outside the package's files: one `[[test]]` stanza in `crates/gcg-search/Cargo.toml`
  (`p6h_pregame`, path `tests/scenarios/p6h_pregame.rs`), because Cargo does not discover test
  files in a subdirectory (interface notes below).

## W-44 — Play/draw choice: policy-vs-policy simulations, biased to going first, capped simulations scored 0.5

- **Status:** resolved (mechanism implemented and measured). Its measured effect is exactly zero
  (non-negative: no game of the row changed), so it is kept available; switching it on in the
  presets is the lead's decision.
- **Resolution as implemented:** `exclude_capped_play_draw`,
  `crates/gcg-search/src/mech/p6h/play_draw.rs`:
  - the reference's games (`first_player_games` seeds drawn up front in the reference's order,
    every "first" game then every "second" game on the same determinisations, temperature 0.5,
    at most 3,000 decisions), through the shared machinery;
  - a pair in which either game is capped (no winner after 3,000 decisions), drawn (winner
    `-1`) or failed (`engine_error_leaf`) is excluded from the paired estimate, so the decision
    (go second only when that is better by more than two paired standard errors, otherwise go
    first) rests on finished games only (W-44's resolution plan);
  - each choice's reported score (the alternative's `value`) scores a capped or failed game by
    the evaluation, the leaf value of its final state, instead of 0.5 (PLAN §12.3 S11 and the
    flag's documentation in `flags.rs`), and a drawn game 0.5;
  - each alternative's `visits` is the number of counted pairs, so the decision trace and the
    decision log record the excluded simulations (`first_player_games - visits` pairs).
  - With every game finished the choice, the alternatives and the generator draws equal the
    reference's bit for bit. The two texts of the plan (WEAKNESSES: "excludes capped or drawn
    simulations from the paired estimate instead of scoring them 0.5"; PLAN §12.3 and
    `flags.rs`: "capped games scored by the evaluation instead of 0.5") are both met: the first
    governs the decision, the second the reported scores (interface notes).
- **Evidence** (all pass):
  - Unit (W-44's "a capped simulation is excluded (strengthened)"):
    `mech::p6h::play_draw::tests::unfinished_games_are_excluded_from_the_pairs_and_scored_by_the_evaluation`
    (the classification of capped, drawn, failed, won and lost games);
    `mech::p6h::tests::capped_games_are_excluded_from_the_pairs_and_scored_by_the_evaluation`
    (the 6 CHOOSE_FIRST vector positions with a 20-decision cap: every game capped, 0 pairs
    counted, the agent goes first, every reported mean an evaluation strictly inside
    (0.05, 0.95) and never 0.5; to the end, every pair counts).
  - Reference pinned at 0.5 ("scored 0.5 (reference, pinned)"):
    `mech::p6h::tests::drawn_games_are_excluded_from_the_pairs_and_the_reference_scores_them_even`
    (turn limit 3, every game drawn: `first_player_choice` counts all 24 pairs with means of
    exactly 0.5; the mechanism counts 0 and goes first) and the scenario test
    `crates/gcg-search/tests/scenarios/p6h_pregame.rs::w44_unfinished_play_draw_games_leave_the_paired_estimate`
    (red-green-zeon vs blue-white-federation, game seed 7, turn limit 20, reference `strong`
    budget: 13 of 48 pairs have a drawn game (14 drawn games); the reference reports all 48
    pairs with drawn games at 0.5, bit for bit equal to the means recomputed from the same games
    played independently in the test; the mechanism reports 35 counted pairs, the same means, and
    the choice of the paired test over those 35 pairs (both go first here)).
  - Null configuration unchanged and equivalence of the finished case:
    `mech::p6h::tests::play_draw_is_the_reference_when_every_game_finishes` (6 vector positions x
    8 and 24 games: action, alternative bits and generator state equal the reference's);
    `tests/null_config.rs` (unchanged, pass).
  - Determinism: `mech::p6h::tests::pregame_choices_are_identical_for_any_worker_count` (18
    vector positions with both flags on, inline and on a 4-worker pool: identical bits).
  - e2e (W-44's "the trace reports go-first rates per matchup and excluded simulations"):
    `gcg-eval head-to-head search:null-standard:exclude_capped_play_draw=true ref:standard
    --decks panel --pairs tuning --games 60 --role dev --seed 20800144 --choose-first agents
    --decision-trace ...`; `docs/dev/eval/P6-H/w44-e2e-head-to-head-dev.json` carries the
    realised play/draw counts per agent for every pair (`by_pair[].result.play_draw`) and
    `docs/dev/eval/P6-H/w44-e2e-trace-summary-dev.json` (from the decision trace by
    `docs/port/eval/P6-H/trace_summary.py`) the choices, go-first counts and counted/excluded
    simulations per pair and agent: each agent chose in 30 games and went first in all 30;
    `search-standard` counted 720 of 720 pairs (0 excluded) and the reference reports 720 (it
    never excludes).
- **Ablation rows** (PLAN §12.4; `--choose-first agents`, because forced balance bypasses the
  decision; reference `standard` budget; 15 tuning pairs; mirrored blocks; dev seeds):

  | Row | wins/N | Wilson 95% | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | file |
  | --- | --- | --- | --- | --- | --- | --- |
  | individual, `search:null-standard:exclude_capped_play_draw=true` vs `ref:standard`, seed 20800044 | 212/400 = 0.530 | [0.481, 0.578] | 0.060 [-0.007, 0.126] | 30/18 | 9,917.6 vs 9,658.0 / 117,992.4 vs 115,571.7 | `docs/dev/eval/P6-H/w44-exclude-capped-play-draw-individual-dev.json` |
  | null control (same seed, flag off) | 212/400 = 0.530 | [0.481, 0.578] | 0.060 [-0.007, 0.126] | 30/18 | identical | `docs/dev/eval/P6-H/w44-null-control-dev.json` |
  | mechanism vs control, game by game | 0 of 400 games changed | — | effect 0.000 | 0/0 | — | `docs/dev/eval/P6-H/w44-paired-vs-control-dev.json` |

  The two rows are identical game for game: in these 400 games no play/draw simulation ended
  unfinished in a way that changed a choice, so the mechanism's measured effect is exactly zero
  (non-negative, null). Play/draw counts: each agent was on the play in 200 of 400 games. The
  row against the reference (0.530) is deal noise of the null configuration on these deals, as
  the control shows. Not confirmed on confirm seeds: a row with zero changed games is not
  promising (PLAN §12.4). Cost: none beyond the reference's simulations (the same
  `2 x first_player_games` games; one evaluation per capped game). The leave-one-out row needs
  the integrated bundle (lead, §12.9 item 4).

## W-45 — Mulligan heuristic: greedy four-turn opening score with a fixed 90% threshold

- **Status:** resolved (mechanism implemented, scenario-tested and measured). Its measured effect
  is non-negative in both the dev and the confirm rows, so it is kept as an adoption candidate;
  switching it on in the presets is the lead's decision.
- **Resolution as implemented:** `paired_mulligan`, `crates/gcg-search/src/mech/p6h/mulligan.rs`,
  on the play/draw machinery (`sim.rs`, W-45's "reusing the play/draw machinery"):
  1. the reference rule (`mulligan_choice`: `opening_score` against 90% of the average of
     `mulligan_samples` fresh hands) runs first with the decision's generator, exactly as the
     reference; its choice is the fallback;
  2. `mulligan_samples` pair seeds are drawn next (96 at the reference `standard` budget, 192 at
     `strong`); on each seed's world, determinised from the observer's information set, KEEP and
     REDRAW are both played (the redrawn hand is the world's own next five, rule 6-2-1-6-1) by
     the strengthened rollout policy for both players at the search's rollout temperature until
     each player has had four turns (`OPENING_TURNS` of `ai/setup.py:22`: stop at turn 9), and
     the final state is scored by the search's leaf value (`Mechanisms::leaf_value`; a game that
     ended scores as a won or lost leaf), so Commands, Bases, effects and the matchup all count;
  3. REDRAW when the paired difference `redraw - keep` is above two paired standard errors, KEEP
     when it is below minus two, the reference rule's choice otherwise (W-45's "otherwise fall
     back to the opening-score rule").
  The alternatives carry the simulated mean values of KEEP and REDRAW with the number of pairs.
  The fixed 90% threshold now decides only when the simulations do not.
- **Scenario test** (T-122; `crates/gcg-search/tests/scenarios/p6h_pregame.rs`, pass, 12.6 s):
  two REDRAW positions from seeded games on tuning decks (no held-out deck), each with its
  ground truth from 2,000 paired full games on independent determinisations (KEEP and REDRAW on
  the same world and rollout generator, both players by the reference rollout policy at
  temperature 0.5 to the end, at most 3,000 decisions), then 16 agent seeds through
  `gcg_search::pregame` on the decider's observation at the reference `standard` (96 samples)
  and `strong` (192 samples) budgets:

  | Test | Position | Hand | Truth `redraw - keep` (2,000 pairs) | Reference right | `paired_mulligan` right |
  | --- | --- | --- | --- | --- | --- |
  | `w45_the_reference_redraws_a_ramp_hand_that_wins_more_kept` | red-purple-control vs blue-green-ramp, game seed 32, seat 1 (on the draw) decides after seat 0 keeps | Strike Rouge (Ootori), Penelope (Middle Form), Re-GZ BWS, Gundam AGE-2 Normal, G-Self (`opening_score` 8.5 against a threshold near 11.3, 90% of the sampled average; it ignores that Re-GZ BWS and G-Self each place an EX Resource) | -0.1405 (se 0.0144, z -9.8): keep | 0/16 at `standard`, 0/16 at `strong` (redraws every time) | 16/16 at `standard`, 16/16 at `strong` |
  | `w45_the_reference_keeps_a_hand_that_wins_more_redrawn` | purple-white-tekkadan vs blue-green-ramp, game seed 2, seat 0 (on the play) | CGS Mobile Worker x2, Isaribi (Base), Gundam Barbatos 4th Form, Shiden Custom (Ryusei-Go) (the two Level 1 Units score early, so the reference rule keeps) | +0.1610 (se 0.0135, z +11.9): redraw | 0/16, 0/16 (keeps every time) | 16/16, 16/16 |

  Both meet W-45's verification (reference wrong, recorded; strengthened right in at least 14 of
  16 seeds). The WEAKNESSES text's example hands ("early Commands and a Base", "a Unit-only hand
  that fails against the matchup") were searched for among REDRAW positions of the tuning decks;
  the two pinned positions are the clearest reference errors found in each direction (one hand
  whose effects `opening_score` ignores, one Unit-heavy hand it overrates against the matchup).
  The mulligan regret of W-45's verification is measured with the W-43 tool below the rows.
- **Other evidence** (unit tests in `crates/gcg-search/src/mech/p6h/tests.rs`, pass):
  `the_mulligan_falls_back_to_the_reference_rule_without_evidence` (12 REDRAW vector positions x
  1 and 96 samples: the generator state is the reference rule's plus one draw per pair; with one
  pair, no standard error, the choice is the reference rule's);
  `mulligan_lines_play_the_opening_turns_on_shared_worlds` (every line stops at turn 9 or ends;
  cost: 6,106 engine applies per REDRAW decision at 96 samples, 12,156 at 192, averaged over the
  12 positions); `pregame_choices_are_identical_for_any_worker_count`;
  `tests/null_config.rs::the_copy_replays_the_reference_pregame_choices` (unchanged, pass).
- **Ablation rows** (PLAN §12.4; reference `standard` budget, 15 tuning pairs, mirrored blocks,
  forced play/draw `seat0`):

  | Row | wins/N | Wilson 95% | paired difference (Newcombe 95%) | discordant | iterations / applies per game (challenger vs opponent) | file |
  | --- | --- | --- | --- | --- | --- | --- |
  | individual, `search:null-standard:paired_mulligan=true` vs `ref:standard`, dev seed 20800045 | 188/400 = 0.470 | [0.422, 0.519] | -0.060 [-0.137, 0.018] | 26/38 | 10,006.2 vs 9,915.5 / 120,761.1 vs 119,326.7 | `docs/dev/eval/P6-H/w45-paired-mulligan-individual-dev.json` |
  | null control (same seed, flag off) | 181/400 = 0.453 | [0.404, 0.501] | -0.095 [-0.168, -0.021] | 20/39 | 9,928.3 vs 9,994.4 / 120,055.6 vs 119,578.3 | `docs/dev/eval/P6-H/w45-null-control-dev.json` |
  | mechanism vs control, game by game (dev) | 60 of 400 games changed; challenger wins in them 25 vs 18 | — | effect on the row +0.0175, Wald 95% [-0.0049, 0.0399] | 14/7 (sign test p = 0.19) | — | `docs/dev/eval/P6-H/w45-paired-vs-control-dev.json` |
  | individual, confirm seed 30800045 (run once) | 836/1600 = 0.522 | [0.498, 0.547] | 0.045 [0.009, 0.081] | 128/92 | 9,884.0 vs 9,907.1 / 119,036.2 vs 119,757.9 | `docs/dev/eval/P6-H/w45-paired-mulligan-individual-confirm.json` |
  | null control (same confirm seed, flag off) | 822/1600 = 0.514 | [0.489, 0.538] | 0.028 [-0.007, 0.062] | 111/89 | 9,863.8 vs 9,937.1 / 119,127.2 vs 119,856.9 | `docs/dev/eval/P6-H/w45-null-control-confirm.json` |
  | mechanism vs control, game by game (confirm) | 173 of 1,600 games changed; challenger wins in them 80 vs 66 | — | effect on the row +0.0088, Wald 95% [-0.0009, 0.0184] | 38/24 (sign test p = 0.098) | — | `docs/dev/eval/P6-H/w45-paired-vs-control-confirm.json` |

  How to read the rows. The individual row against `ref:standard` carries the deal noise of the
  null configuration itself: on the dev seed the null control loses to the reference by
  -0.095 [-0.168, -0.021], on the W-44 dev seed it wins by +0.060, and on the confirm seed by
  +0.028, so the row's distance from 0.5 is mostly the deals. The null control plays the same
  deals under the same agent name, so the games in which the mechanism changes no decision are
  replayed exactly (340 of 400 and 1,427 of 1,600 games identical, row by row), and the
  game-by-game comparison isolates the mechanism: +1.75 percentage points of the row's win rate
  on dev and +0.88 on confirm, positive in both, not significant at 95% in either (the confirm
  interval reaches -0.09 points). Pooled over both runs, which was not pre-registered, the discordant counts are
  52/31 (sign test p = 0.028). The mechanism changed a REDRAW decision in about one game in nine
  (173 of 1,600 games on confirm).

  Mulligan regret (W-43 `gcg-eval regret`, W-45's "mulligan regret via the W-43 tool"): 120
  REDRAW positions sampled from 60 `ref:standard` self-play games on the tuning pairs (dev seed
  20800245, both runs on the same positions), each candidate against the oracle
  `search:null-strong:paired_mulligan=true,mulligan_samples=768` (8x the pairs), the two choices
  played out to the end in 64 shared worlds by the playout policy at temperature 0.5:

  | Candidate | positions differing from the oracle | mean regret per decision | 95% interval | file |
  | --- | --- | --- | --- | --- |
  | `ref:standard` (the reference rule) | 27 / 120 | 0.0173 | [0.0079, 0.0267] | `docs/dev/eval/P6-H/w45-regret-reference-dev.json` |
  | `search:null-standard:paired_mulligan=true` | 15 / 120 | 0.0046 | [-0.0004, 0.0095] | `docs/dev/eval/P6-H/w45-regret-paired-mulligan-dev.json` |

  The oracle is the same estimator with more pairs, so the mechanism's low regret is partly by
  construction; the reference rule's regret is the independent part: in full-game playouts the
  oracle's choice beats the reference rule's by 1.7 percentage points per decision, with an
  interval that excludes 0.

  Cost: about 6,100 engine applies per REDRAW decision at the reference `standard` budget (about
  5% of the 120,000 search applies per game per seat) and 12,200 at `strong`. These applies are
  not in the rows' applies per game: pre-game decisions report no `SearchStats`, so the reference's
  play/draw simulations are not counted either (interface notes). The leave-one-out row needs the
  integrated bundle (lead, §12.9 item 4).

## Interface notes for the lead

1. **Test target outside the package's files.** `crates/gcg-search/Cargo.toml` gains one
   `[[test]]` stanza (`name = "p6h_pregame"`, `path = "tests/scenarios/p6h_pregame.rs"`): Cargo
   discovers only `tests/*.rs` and `tests/*/main.rs`, so a file in `tests/scenarios/` builds only
   when registered. Other packages' `tests/scenarios/<pkg>_*.rs` files need the same (or one
   shared `tests/scenarios/main.rs`, which no package owns). The file reaches the shared helpers
   with `#[path = "../common/mod.rs"] mod common;`.
2. **Pre-game effort is not counted.** `Mechanisms::pregame` returns only `(Action,
   Vec<Alternative>)` and `agent.rs` reports `search: None` for REDRAW and CHOOSE_FIRST, so the
   rows' "applies per game" leave out every pre-game simulation, the reference's play/draw games
   included. PLAN §12.3 says "simulations count as applies". Proposed change (lead-owned files):
   `PregameCx` gets an effort sink (or `pregame` returns a `gcg_ai::SearchEffort`) and `agent.rs`
   reports `SearchStats { iterations: 0, effort: { applies_aux, .. }, .. }` for pre-game
   decisions; `sim::Line::applies` already counts each line's applies. Measured meanwhile: 6,106
   and 12,156 applies per REDRAW decision (unit test above).
3. **W-44's two texts.** `flags.rs` documents `exclude_capped_play_draw` as "Capped play/draw
   simulations scored by the evaluation instead of 0.5" (as PLAN §12.3 S11); WEAKNESSES' binding
   plan says capped or drawn simulations are excluded from the paired estimate. The
   implementation does both (the decision excludes, the reported means use the evaluation); the
   flag's doc line could say so. No contract file was edited.
4. **Ablation rows and deal noise.** An individual row against the reference also measures the
   null configuration's luck on its deals (three null controls here: -0.095 [-0.168, -0.021],
   +0.060 [-0.007, 0.126], +0.028 [-0.007, 0.062]; the first excludes 0). A mechanism that
   changes few decisions is measured far more precisely against a null control on the same seed,
   game by game (`docs/port/eval/P6-H/pair_rows.py`). A `--control` option of `gcg-eval
   ablation` that plays and pairs the control row would make this standard.
5. **Seeds and evidence.** Ledger entries (9, dev 20800044, 20800045, 20800144, 20800245;
   confirm 30800045) are in `docs/dev/eval/P6-H/seed-ledger.json`, to be merged into the
   shared ledger. The reports are not in the evidence manifest (`gcg-eval evidence add` writes a
   file outside this package); `docs/dev/eval/P6-H/README.md` lists the exact commands. The
   commit cited there and here (86e481d) must stay reachable for `gcg-eval evidence check`.
6. **tests.json.** T-122 ("Scenario test W-45 mulligan hands") can be marked passing (lead-owned
   file).
7. **Machine load.** The rows ran while other agents loaded the machine (load average about
   150); results are deterministic and unaffected, only slower.

## Not done in this package

- Leave-one-out rows for both flags: they need the integrated bundle (PLAN §12.4, §12.9 item 4,
  the lead's step).
- A W-44 confirm row: the dev row changed no game, so it is not promising (PLAN §12.4).
- The strengthened rollout policy of W-44's and W-45's plans: P6-F's `calibrated_policy` is not
  in this build; the simulations call `Mechanisms::rollout_choice`, so they use it as soon as the
  bundle switches it on, and the rows above then need re-measuring in the bundle.

## Gate (run after the last code change, P6-H worktree at e6a6204, squashed into commit 83a06c8)

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --release` (`--no-fail-fast`) | 989 passed, 1 failed before this file existed: `gcg-eval` `phase6::w127_the_port_evaluation_docs_cite_only_reachable_commits`, whose evidence check then found the confirm reports cited in `docs/dev/eval/P6-H/README.md` not yet written; rerun after the reports landed: see the final line |
| `gcg-diff corpus --tier 1 --require-all` (the gate built into `CARGO_TARGET_DIR=target/gate`, so the measurement runs kept their `target/release` binary) | 3804/3804 |
| `target/release/gcg-diff lints` | lints passed |
| `COMPARE_OUT=build/p6h-equivalence scripts/compare_python_vs_rust.sh equivalence --matches 2 --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 --decision-logs off` | 3/3 identical (fast 1/1, standard 1/1, strong 1/1) |
| `uv run ruff check docs/dev/eval/P6-H/` and `uv run ruff format --check docs/dev/eval/P6-H/` | clean |
| `cargo test -p gcg-eval --release` after the reports and this file landed (the only crate whose tests read these documents) | 91 passed, 0 failed (`w127_the_port_evaluation_docs_cite_only_reachable_commits` passes); with the 989 above: 990 of 990 |
| `target/release/gcg-eval evidence check` | exit 0 |
