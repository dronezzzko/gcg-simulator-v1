# Weaknesses, limitations and deferred proposals: resolution plan for the Rust port

This file lists every known weakness, limitation, rejected-but-recorded idea and deferred
proposal of the Python gcg-sim reference (branch `rust-port` at `d9d41af`), each with a concrete
resolution plan for the Rust port. Four independent sweeps (docs, code, git history, untracked
artifacts) produced 389 raw items; items that describe the same weakness, or one family of
interpretations or fixed bugs that shares one resolution plan, were merged into the first 138
entries, keeping every source of every merged item. A completeness review then found 18 more
items. Sixteen are new entries (W-139 to W-154, in the section "Later additions"). The other two
were merged: the C math library dependence into W-137, and the mirrored-block docstring into
W-129. The same review corrected W-12, W-20, W-32, W-35, W-38, W-42, W-63, W-64, W-67, W-76,
W-100, W-111, W-112, W-113, W-117, W-131, W-133 and W-137. That makes 154 entries. A second
completeness review found ten more items, now W-155 to W-164 (also in "Later additions"). One
of its claims did not hold and is corrected in W-164: the conflict-id card-number inference is
live, not unused. Another was changed in W-162: the proposed `host_seq` counter would fire on
every 【Destroyed】 trigger. The second review also corrected W-05, W-17, W-35, W-75, W-76, W-93,
W-98, W-120 and W-137, and W-27 and W-60 gained cross-references. That makes 164 entries.

**How this file was checked.** Every cited source was opened. A check script confirmed, on
2026-11-01, that every cited path and line range exists, every cited commit resolves, and every
quote occurs in one of its entry's sources (in a commit message or a cited `sha:path` for
history, by exact bytes for bytecode). Commits lost in the squash of PR #5 are named in the text
(W-127), never cited as sources. Claims the sweeps marked plausible were checked against the code
(W-09, W-32, W-47, W-68). One sweep claim could not be confirmed and is marked as such (W-20).
W-139 is inferred from the search design and has not been measured; it says so. The later
additions were checked the same way, and their reproductions were run on 2026-11-01 (W-143,
W-144, W-154). The second review's items were checked the same way on 2026-11-01. W-155's
classification was reproduced: a reworded GD05-089 parenthetical is classed `typography`, with
an unchanged `text_hash`. The counts in W-155, W-157, W-158, W-163 and W-164 come from scans of
the packaged data and registry run that day. W-159 and W-162 are inferred from the code and
have not been reproduced; they say so. After these edits the check was rerun on all 164
entries. Every cited path exists and every cited commit resolves. Every quote occurs inside its
cited line range, commit message or bytecode file, with whitespace normalised across line
wraps. That was not true before: W-120's quote lay outside its cited range.

**Ground rules for the plans.**

- *Reference*: the ISMCTS (with greedy and random) reproduced byte for byte from Python,
  reachable only through `gcg-eval`. It never changes. Where an entry says the reference keeps a
  behaviour, the port reproduces it exactly, and tests pin it.
- *Strengthened search*: behind the presets `standard` (strongest; node budgets start from
  reference `strong`) and `fast` (screening; node budgets start from reference `standard`).
  Strength changes live in the AI only. Rules, card behaviour and data stay unchanged. Everything
  is deterministic given the seed, and budgets are node counts, never wall-clock time.
- *Non-AI fixes* (engine, validator, reporting, runner, tooling, performance, docs) are made in
  Rust only when they change no rules interpretation, card data or packaged source.
  Assumption: they also keep the Python-equivalent outputs byte-identical by default
  (`results.json`, `summary.md`, `games.ndjson`, replays, as compared through
  `scripts/normalize_outputs.py`; today that script compares `results.json` only up to JSON
  formatting, and W-160 makes the comparison byte-exact). New information goes into additive
  outputs (`analysis.json`, `gcg-eval` traces, diagnostics files) or behind opt-in flags. W-100
  is the one plan that needs the lead's decision on this. W-137 needs the lead's decision on
  something else: a scoped `unsafe` exception for libm `erfc`.
- *Admissible exceptions*: only the reference's explicit design decisions on multiplayer,
  sideboarding and clock (W-131 to W-133). Every other entry has a plan; its status line records the Phase 6 outcome (see "Phase 6 outcomes" below).
- Changes outside the port's own files are proposed, not made: Python docs, packaged data
  wording, `.claude/settings.json` and the deletion of untracked bytecode are listed for the lead
  or the user.

**Resolution types** (table column): *AI (strengthened search)*: a mechanism in the strengthened
presets only. *eval tooling*: a `gcg-eval` subcommand or measurement. *Rust fix*: an
implementation change that keeps outputs identical or only adds outputs. *port-preserve*: the
behaviour is kept exactly and the entry adds tests, tripwires or reporting. *data tripwire*: a
data-gate test that fails when a future card or refresh breaks a precondition. *tooling*, *docs*:
port scripts and docs. *admissible exception*: a design decision of the reference.

**Verification kinds.**

- *Scenario test*: a deterministic position built with the Rust testkit (port of
  `gcg_sim.testkit.Scenario`): fixed decklists, fixed true hidden cards, agent seeds 1..N (N = 16
  unless stated). The test records the reference's solve count, which must miss the stated bar
  for the scenario to count as a reproducer, and asserts the strengthened preset's bar.
  Reference runs go through `gcg-eval`. Bars are initial targets, fixed when the scenario is
  built and the reference's failure rate has been measured.
- *Ablation row*: a `gcg-eval` head-to-head in mirrored blocks of four (two deals, each played
  twice with the agents swapped between seats, W-42), on the AI test decks and
  the held-out deck panel (W-20), with development seeds for screening and one held-out seed run
  once for confirmation. Each mechanism gets an *individual* row (the reference configuration at
  the same node budget plus only this mechanism, against the reference) and a *combined* row (the
  full strengthened preset against the same preset without this mechanism, leave-one-out). The
  bundle rows are strengthened `standard` against reference `strong` and strengthened `fast`
  against reference `standard`. Every row reports wins/N, the Wilson 95% interval, the paired
  difference with a Newcombe 95% interval (W-42), discordant counts and nodes per game. Adoption
  follows the existing rule (`docs/ASSUMPTIONS.md:92`): decision-level or scenario evidence of a
  real fix, no large harm in a 400-game guard, and the bundle measured at game level.
- *Unit*, *e2e*, *fuzz*: Rust tests (unit, end-to-end CLI or `gcg-eval` runs, randomized
  invariant runs). A *negative control* shows a check can fail. *Pinned* means the reference's
  behaviour is asserted as an expected value.

`gcg-eval` subcommand names below (`budget-curve`, `fidelity`, `weakness`, `regret`, `tune`,
`calibrate`, `compare-runs`, `transcripts`) are proposals.

**Phase 6 outcomes (PLAN §12.10).** Each entry ends with its outcome. The outcomes are copied from the package records in `docs/port/eval/weakness-status/` (P6-A to P6-O, P6-decks and M15-bundle) and checked against the repository on 2026-11-02 at HEAD `d750b6c`. The items that were open then were updated on 2026-11-04 at HEAD `a926ac7`, whose crates and evaluation records are those of `d20f04b` (`a926ac7` adds the dist build only). Their new outcomes come from the gap-closing records (GAP-A1, GAP-A2, GAP-B, GAP-C and GAP-D in the same directory, commit b8483dc) and from M15b (`docs/port/eval/M15b/README.md`, commit 8c89b5e). Each outcome has three fields:

- **Status.** The status is one of:
  - `resolved`;
  - `resolved by measurement (not adopted)`: a strengthened-search mechanism that is implemented and measured null or negative, and stays off in the shipped presets;
  - `admissible exception` (W-131 to W-133);
  - `open`, followed by the remaining work.

  The summary table's Status column abbreviates these. `docs/port/eval/weakness-gaps.md` lists every open item with what closes it.
- **Evidence.** The tests and runs that support the status, with their register id `T-NNN` from `docs/port/tests.json` where one exists.
- **Ablation.** The rows of PLAN §12.4.

The final strength evaluation (E1-E4, §12.6) runs twice. The first ran on the M15 presets with eval masters 90200001-90200006. It was too close to the bars, and its 27 panel losses to random were not documented with replays, so it was not accepted (`docs/port/eval/final-1/README.md`, `docs/port/progress.md`). The second ran on eval masters 90300001-90300006 with the M15b presets (`docs/port/eval/final-2/README.md`): E2 and E3 pass (all 14 panel losses to random documented with verified replays), E1 passes the brief's bar but misses the plan's held-out pooled bound by 0.002, and E4 passes on the panel but not on the AI test decks (`docs/port/eval/notes.md`).

*Shipped presets.* The M15 presets are recorded in `crates/gcg-search/src/presets.rs` and `docs/port/eval/M15/README.md`, and M15b's change in `docs/port/eval/M15b/README.md`:

- `standard` is 8x the reference `strong` budget: `iterations_per_option` 1024, `min_iterations` 2560, `max_iterations` 10240.
- `fast` is 1.5x the reference `standard` budget: 96 / 240 / 960.
- Both enable `adaptive_horizon`, `capped_rollout_continuation`, `tactical_solver`, `exclude_capped_play_draw` and `engine_error_leaf` (M15).
- M15b added `kill_aware_targeting` (W-26) and `tree_lethal_select` (W-25) to both, after the first final evaluation and on tuning-pair evidence only. Budgets and every other setting stay M15's. Every other mechanism is off (`presets.rs` `SHIPPED_MECHANISMS`, test `shipped_presets_enable_the_m15b_bundle`).

*Rows.* Every row runs `gcg-eval ablation`, `head-to-head` or `budget-curve` on the 15 tuning pairs of `docs/port/eval/panel.json`. It plays mirrored blocks of four with forced play/draw (`--choose-first seat0`, A24) unless the row says `agents`. It uses dev-role masters, and confirm-role masters for confirmations, which are run once. A row reports:

- wins/N, with the rate and its Wilson 95% interval;
- the paired difference, with its Newcombe 95% interval;
- the McNemar discordant counts;
- the iterations and the applies per game, challenger vs opponent;
- its evidence file.

The row kinds are:

- *Individual*: `search:null-standard` (the reference `standard` budget, every mechanism off) plus the mechanism, against `ref:standard`.
- *High budget*: the pre-M15 shipped `search:standard` at twice its budget (256 / 640 / 2560) with the mechanism, against the same configuration without it. That configuration is the reference `strong` settings with `engine_error_leaf`.
- *vs placebo* or *vs null*: the row compared game by game with a no-op row on the same master (Wald 95%).
- *Bundle* (M15, dev 21500100, at twice the budget, against the null configuration):
  - `B_core` is `adaptive_horizon`, `capped_rollout_continuation` and `tactical_solver`. It scored 217/400. Confirmed (confirm 30150100): 854/1600 (0.534, [0.509, 0.558]), paired +0.068 [+0.030, +0.105], discordant 148/94, 32329 vs 32394 / 595018 vs 373624 (`docs/port/eval/M15/rows/confirm-bcore-vs-null2x-1600.json`).
  - `B_mid` is `B_core` plus 14 cheap mechanisms that were null alone: `ex_variants`, `typed_fingerprint`, `prune_worlds=4`, `branch_all_selects`, `rest_prune_worlds`, `target_rollouts`, `real_idle_line`, `modal_lookahead`, `burst_decline_list`, `exhaustive_polarity`, `transpositions`, `macro_multi_pick`, `leaf_density` and `exclude_capped_play_draw`. It scored 212/400; against `B_core` on the same deals, -0.013 [-0.061, +0.036].
  - `B_full` is `B_mid` plus `subtree_reuse`. It scored 193/400; against `B_core`, -0.060 [-0.111, -0.009].
- *Leave-one-out* (dev 21500200): the adopted bundle against itself without one mechanism. It is run for every adopted mechanism (§12.9 step 4). A mechanism that is not adopted has no leave-one-out row ("not applicable"); its bundle evidence is `B_mid` or `B_full` where it was in them.

*"No AI effect" rows (A13).* The common rows below give their evidence once. An entry cites its row by letter.

- **Row J** (P6-J): engine code the AI executes, built in with no off switch. The change alters no value the engine or either search reads in a release build. With the change:
  - `gcg-diff corpus --tier 1 --require-all` gives 3804/3804, and `--tier 2 --require-all` gives 4154/4154. Both include the `search` and `baselines` sets.
  - The seed-1 equivalence subset is 3/3 identical (reference `fast`, `standard`, `strong`).
  - The gcg-search scenario and null-configuration tests pass.
  - Source: `docs/port/eval/weakness-status/P6-J.md`.
- **Row K** (P6-K): tests, port-side tooling and documents only, with no `crates/*/src` change. The tier-1 corpus gives 3804/3804, including the `search` and `baselines` sets (`docs/port/eval/weakness-status/P6-K.md`).
- **Row L** (P6-L): deck validation, deck-file hints and `gcg-eval validate`, none of which runs during a game. The tier-1 corpus gives 3804/3804: the `search` set 9/9 with 771 agent decisions and the `baselines` set 8/8 with 625. The equivalence smoke gives 3/3 (`docs/port/eval/weakness-status/P6-L.md`).
- **Row M** (P6-M): no code the AI executes during a decision or a game. The engine, effects, agents, both searches and the runner's game loop are untouched. The tier-1 corpus gives 3804/3804, the equivalence smoke 3/3, and T-153 byte identity holds (`docs/port/eval/weakness-status/P6-M.md`).
- **Row I** (P6-I): evaluation tools in `crates/gcg-eval/` only, which read reports or play the existing agents unchanged. The tier-1 corpus gives 3804/3804 (`docs/port/eval/weakness-status/P6-I.md`).
- **Row N** (P6-N): gate tools (`gcg-diff`, never shipped, A31), the dev-only `testkit` of `gcg-engine`, scripts and documents. The tier-1 corpus gives 3804/3804 and the equivalence smoke 3/3 (`docs/port/eval/weakness-status/P6-N.md`).

*Where options live.* Every option that a resolution adds lives in `gcg-eval` (A30); `gcg-sim`'s command line is unchanged (T-152). An entry whose plan named a `gcg-sim` flag records the option's actual location under "Option location". Of the proposed subcommand names above, `budget-curve`, `fidelity`, `regret`, `tune`, `calibrate` and `compare-runs` exist since Phase 6. `weakness`, `transcripts` and `data-status` exist since the gap closing (b8483dc; W-17, W-119, W-88, W-145). `target/release/gcg-eval --help` lists all nine.

## Summary

| ID | Title | Category | Resolution type | Verification | Status |
| --- | --- | --- | --- | --- | --- |
| W-01 | Two-turn rollout horizon: plans that pay off later rest on the evaluation | ai-search | AI (strengthened search) | scenario + ablation | resolved |
| W-02 | Leaf-value constants cap the horizon at four turns | ai-search | AI (strengthened search) | unit | resolved (not adopted) |
| W-03 | Lethal-threat defence: the audited "keep the last Blocker" position is unsolved; threat-aware rollouts were measured null and removed | ai-search | AI (strengthened search) | scenario + ablation | resolved |
| W-04 | Budget plateau: strength is model-limited past about 2x budget; `strong` is barely stronger than `standard` | ai-search | AI (strengthened search) + eval tooling | ablation + unit | resolved |
| W-05 | Strategy fusion: opponent nodes pool statistics across hidden worlds (single-observer ISMCTS) | ai-search | AI (strengthened search) | scenario + ablation | resolved (not adopted) |
| W-06 | Observation keys count one child twice when the opponent passes twice in a row | ai-search | AI (strengthened search) | unit + ablation | resolved (not adopted) |
| W-07 | Determinization samples hidden cards uniformly: no inference from the opponent's actions | ai-search | AI (strengthened search) | scenario + ablation + unit | resolved (not adopted) |
| W-08 | Determinization pads or trims pools when effects move cards between players' decks | ai-search | AI (strengthened search) + Rust fix (diagnostic) | unit + fuzz | resolved (not adopted) |
| W-09 | Move canonicalization never pays with an EX Resource on purpose | ai-search | AI (strengthened search) | scenario + unit + ablation | resolved (not adopted) |
| W-10 | Equivalent-move merge fingerprint is conservative and depends on instance numbers | ai-search | AI (strengthened search) | unit + property | resolved (not adopted) |
| W-11 | Pruning and merging run only at the root; no transposition or tree reuse | ai-search | AI (strengthened search) | ablation + unit | resolved (not adopted) |
| W-12 | Root pruning is judged in one fixed determinization by the deterministic policy, with silent caps | ai-search | AI (strengthened search) | scenario + unit + ablation | resolved (not adopted) |
| W-13 | The "while rested" Rest-target exception was added after the measurements and never fired at game level | ai-search | AI (strengthened search) + eval tooling | scenario + e2e | resolved |
| W-14 | Close target choices stay noisy: the <Repair> ping puzzle is solved about a third of the time | ai-search | AI (strengthened search) | scenario + ablation | resolved (not adopted) |
| W-15 | Undocumented rollout cap of 250 decisions: capped rollouts are evaluated mid-turn | ai-search | AI (strengthened search) + eval tooling | unit + e2e | resolved |
| W-16 | Budget is fixed by option count; only effect-target decisions may extend | ai-search | AI (strengthened search) | scenario + ablation | resolved (not adopted) |
| W-17 | Remaining implausible plays: passivity, questionable attacks, missed lethals; the audit sample is small | ai-search | AI (strengthened search) + eval tooling | scenario + ablation | resolved |
| W-18 | `fast` rates are not `standard` rates; fidelity rests on one weakened deck | ai-search | AI (strengthened search) + eval tooling | e2e | open (verification not met) |
| W-19 | Strength acceptance bars are thin floors | ai-search | eval tooling (acceptance gates) | e2e | resolved |
| W-20 | Strength evidence rests on few decks, and the tuning decks overlap the example decks | ai-search | eval tooling (deck panel) | unit + e2e | resolved |
| W-21 | The decision log keeps the top 5 alternatives and does not record search effort | ai-search | eval tooling (instrumentation) | unit + e2e | resolved |
| W-22 | Greedy baseline: one determinization, baseline knobs, fixed choices, silent cap | ai-policy | port-preserve + eval tooling | e2e + unit | resolved |
| W-23 | Hand-written playout policy: Commands under-cast, abilities over-activated, unfitted constants | ai-policy | AI (strengthened search) | ablation + e2e | resolved (not adopted) |
| W-24 | Modal choices are scored flat | ai-policy | AI (strengthened search) | scenario + unit + ablation | resolved (not adopted) |
| W-25 | The opponent is modelled with the searcher's own policy | ai-policy | AI (strengthened search) | scenario + ablation | resolved |
| W-26 | Kill-aware and damage-payoff targeting were measured and removed | ai-policy | AI (strengthened search, re-measure) | ablation | resolved |
| W-27 | YES_NO answers say no about 12% of the time; the policy always goes first and keeps | ai-policy | AI (strengthened search) | unit + ablation | resolved (not adopted) |
| W-28 | End-phase action-step plays are under-explored by rollouts | ai-policy | AI (strengthened search) | scenario + ablation | resolved (not adopted) |
| W-29 | No policy scorer for ORDER_TRIGGER decisions | ai-policy | AI (strengthened search) | scenario + unit | resolved (not adopted) |
| W-30 | Target polarity classification is coarse | ai-policy | AI (strengthened search) | unit + ablation | resolved (not adopted) |
| W-31 | Heuristic card and fight values are rough | ai-policy | AI (strengthened search) | unit + ablation | resolved (not adopted) |
| W-32 | AI Blocker counts ignore CANT_BLOCK and NO_BLOCKER_VS | ai-policy | AI (strengthened search) | unit + scenario | resolved |
| W-33 | AI module-level caches are unbounded and not invalidated on script override | ai-policy | Rust fix (no behaviour change) | unit | resolved |
| W-34 | Evaluation fitted on pre-change search games; refit and per-preset fits deferred | ai-evaluation | AI (strengthened search) | ablation + e2e | resolved (not adopted) |
| W-35 | The evaluation is linear; interaction terms and other candidate features gained nothing | ai-evaluation | AI (strengthened search) | ablation + unit | resolved (not adopted) |
| W-36 | The evaluation was trained on three tuning decks and transfers weakly to other decks | ai-evaluation | AI (strengthened search) + eval tooling | e2e | resolved (not adopted) |
| W-37 | The evaluation ignores most keywords, card text and effects | ai-evaluation | AI (strengthened search) | unit + ablation | resolved (not adopted) |
| W-38 | can_kill and race features are crude board heuristics | ai-evaluation | AI (strengthened search) | unit + ablation | resolved (not adopted) |
| W-39 | The negative tempo weight is an artifact of when positions are recorded | ai-evaluation | port-preserve + AI (strengthened search) | unit | resolved |
| W-40 | Weight-tuning methodology limits | ai-evaluation | eval tooling | unit | resolved |
| W-41 | Matchup estimates are not calibrated against real tournament results | ai-evaluation | eval tooling (data from the user) | e2e | resolved |
| W-42 | No paired analysis of same-seed runs; compare reports a pooled Wilson interval | reporting | eval tooling | unit + e2e | resolved |
| W-43 | Decision-level regret (oracle replay) tooling was never committed | tooling | eval tooling | unit + e2e | resolved |
| W-44 | Play/draw choice: policy-vs-policy simulations, biased to going first, capped simulations scored 0.5 | ai-setup | AI (strengthened search) | unit + e2e | resolved |
| W-45 | Mulligan heuristic: greedy four-turn opening score with a fixed 90% threshold | ai-setup | AI (strengthened search) | scenario + ablation | resolved (not adopted) |
| W-46 | Safety caps: 200 turns and 20,000 decisions share one end reason; the decision cap is untested; self-play stops earlier with no end reason | engine | Rust fix (additive diagnostics) | unit + e2e | resolved |
| W-47 | No guard against decision-free loops inside advance() | engine | Rust fix (error path only) | unit + fuzz | resolved |
| W-48 | The derived-view fixpoint is silently capped at six passes | engine | Rust fix (diagnostic, identical output) | unit + fuzz | resolved |
| W-49 | The derived-view cache relies on manual touch() calls and a heuristic key; only the paranoid guard checks it | engine | Rust fix (by construction) | unit + fuzz | resolved |
| W-50 | The listened-events fast path depends on the decklists; the listen cache is process-global | engine | Rust fix (by construction) | unit + fuzz | resolved |
| W-51 | The rules-management HP shortcut depends on a registry-global "no HP-lowering effect" flag | engine | Rust fix (identical output) | unit + fuzz | resolved |
| W-52 | GameState.clone shares pending, the derived view and history entries (CB-1) | engine | Rust fix (by construction) | unit + e2e | resolved |
| W-53 | Registry indices are process-local, so saved states do not reload faithfully in another process | engine | Rust fix (content-addressed ids) | unit + e2e | resolved |
| W-54 | Dead enum values: the PAYMENT decision kind and Step.ATTACK_TRIGGERS | engine | Rust fix (no behaviour change) | fuzz | resolved |
| W-55 | Interpretations and N/A rules that hold only for the current card pool | engine | port-preserve + data tripwire | unit + negative control | resolved |
| W-56 | Rules-interpretation decisions the port must keep, pinned by rulings and curated conflicts | engine | port-preserve + conformance tests | unit + differential | resolved |
| W-57 | Historical engine, validator, runner and tooling bugs fixed in Python: regression risk for the port | engine | port-preserve + regression tests | unit + meta-test | resolved |
| W-58 | Most-defensible readings of ambiguous card text and major ruling overrides | effects | port-preserve + reporting | unit + e2e | resolved |
| W-59 | Pilot text routing is inferred, not encoded in the data | effects | port-preserve + audit table | unit | resolved |
| W-60 | Explanatory parentheticals are stripped but carry semantics the bindings must honour | effects | port-preserve + tests | unit | resolved |
| W-61 | Grammar slips and duplicated token clauses in card text break templates | effects | port-preserve + tests | unit | resolved |
| W-62 | 'instead' wording: conditional overrides vs true substitutions | effects | port-preserve + tests | unit | resolved |
| W-63 | The regex compiler covers about 64% of non-vanilla cards; 302 per-card bindings; new sets need manual multi-agent work | effects | general templates, golden-identical (Rust or Python side, after the IR decision) | unit + differential | resolved |
| W-64 | Card-specific custom hooks, several duplicated; custom steps cannot request choices | effects | Rust fix (consolidated hooks, behaviour-identical; form depends on the IR decision) | unit + differential | resolved |
| W-65 | The keyword and timing vocabulary is duplicated across many files | effects | Rust fix (single source) | unit | resolved |
| W-66 | The compiler miscompiled some cards silently; bindings fixed the cases found | effects | Rust fix (differential audit) | unit | resolved |
| W-67 | No templates for "all players each …" and related multi-player-step wording | effects | general templates, golden-identical (Rust or Python side, after the IR decision) | unit + differential | resolved |
| W-68 | Stale "compiled to printed HP" binding notes: Stat.HP already means current HP | effects | Rust fix (dedupe, behaviour-identical) | unit | resolved |
| W-69 | The registry build swallows binding errors; the card silently becomes unimplemented | effects | Rust fix (fail fast) | unit | resolved |
| W-70 | Historical compiler and DSL bugs fixed in Python: regression risk for the port | effects | port-preserve + regression tests | unit + meta-test | resolved |
| W-71 | The ST02/ST05 unmodified starter-deck exceptions are not applied | validator | Rust fix (deferred until data exists) + reporting | unit | resolved |
| W-72 | One current B&R list: no history and no region | validator | Rust fix (schema-ready, data unchanged) | unit | resolved |
| W-73 | Vanilla banned pair: the predicate and the enumerated list can diverge, and divergence only adds a note | validator | data tripwire | unit + negative control | resolved |
| W-74 | Validator predicates that hold only for today's pool: "without effects", primary-name matching, one colour per card | validator | data tripwire | unit + negative control | resolved |
| W-75 | The B&R list is enforced by default although the Comprehensive Rules never mention it | validator | port-preserve + reporting | unit + e2e | resolved |
| W-76 | Tournament-only deck rules, limited formats, tournament procedures and Edition Beta legality are not modelled | validator | reporting (explicit scope, additive outputs) | unit + e2e | resolved |
| W-77 | Deck file format limits | validator | Rust fix (diagnostics) | unit | resolved |
| W-78 | Legality depends on simulator support (UNIMPLEMENTED) | validator | port-preserve + reporting | unit | resolved |
| W-79 | Official errata missing from gcg-api are applied by hand (ST12-001) | data | data tripwire (data unchanged) | unit + negative control | resolved |
| W-80 | Card text format defects corrected by overrides (GD02-053, GD03-015, GD03-035) | data | data tripwire | unit | resolved |
| W-81 | gcg-api structure fields (keyword_effects, timing_markers, link_refs) are lossy | data | port-preserve + test | unit | resolved |
| W-82 | A null printed AP is read as 0 (inference); EX Base AP is null in the source | data | data tripwire | unit + negative control | resolved |
| W-83 | Edition Beta stat records are defective (one-row shift in the source listing) | data | data tripwire | unit | resolved |
| W-84 | Link, name and trait anomalies in the card data | data | port-preserve + test | unit | resolved |
| W-85 | Derived data files are curated by hand; curated conflict texts drifted from their resolutions | data | data tripwire | unit + negative control | resolved |
| W-86 | Printing text divergences resolved to the base printing; a wrong rule citation was fixed in review | data | data tripwire | unit | resolved |
| W-87 | The Comprehensive Rules file has wrong cross-references and typos (the source is never edited) | data | port-preserve + test | unit | resolved |
| W-88 | Provenance gaps: some official sources were read but not stored; cards.json is omitted from the package | data | reporting (provenance listing) | e2e | resolved |
| W-89 | dataset_version's sha is the code commit that produced the data, not the data commit | data | port-preserve + docs | unit | resolved |
| W-90 | Per-card and benchmark-card statistics are correlations inside AI-vs-AI games | reporting | Rust fix (additive analysis) | unit + e2e | resolved |
| W-91 | Results are specific to the AI and preset, and not comparable across AI versions | reporting | Rust fix (guard in comparison tools) | unit | resolved |
| W-92 | Game-level intervals and tests treat games within a BO3 match as independent | reporting | Rust fix (additive analysis) | unit | resolved |
| W-93 | Draws count as non-wins | reporting | port-preserve + reporting | unit | resolved |
| W-94 | Hypothesis error control is per kind of comparison; thresholds are hand-set | reporting | Rust fix (additive analysis) | unit | resolved |
| W-95 | "Never played" signals have no significance test or cap, and cost discards count as never played | reporting | Rust fix (additive analysis) | unit | resolved |
| W-96 | Benchmark-card "seen" covers only played, Burst-revealed or public cards; lift is one-sided | reporting | Rust fix (additive analysis) | unit | resolved |
| W-97 | The results schema file name stays v1 at version 1.1.0 | reporting | port-preserve + docs | unit | resolved |
| W-98 | Distribution statistics are integer nearest-rank values and rounded rates | reporting | port-preserve + Rust fix (additive analysis) | unit | resolved |
| W-99 | Decision logs make outputs large; reports and logs truncate | reporting | Rust fix (opt-in compression) | unit + e2e | resolved |
| W-100 | The summary's reproduce command omits the agent factory and can name a deck that is not a path | reporting | Rust fix | e2e | resolved |
| W-101 | conflict_resolutions lists only resolutions whose id names a deck card | reporting | Rust fix (additive field) | e2e | resolved |
| W-102 | The report directory is not updated atomically as a set | reporting | Rust fix | unit | resolved |
| W-103 | code_digest hashes every file of the installed package; the version can be "unknown" | reporting | Rust fix | unit | resolved |
| W-104 | timing.json is wall-clock based and the only non-reproducible report | reporting | Rust fix (additive fields) | unit | resolved |
| W-105 | Interrupted or failed runs discard all work: no checkpoint or resume | runner | Rust fix | e2e | resolved |
| W-106 | Worker termination: waitpid race, a private executor attribute, AI tools without interrupt handling | runner | Rust fix (by construction) | unit + e2e | resolved |
| W-107 | Spawn start-method constraints on the Python API | runner | Rust fix (not applicable by design) | unit | resolved |
| W-108 | The GCG_SIM_AGENT_FACTORY test hook can replace the AI in a benchmark | runner | Rust fix | unit + e2e | resolved |
| W-109 | Simultaneous defeat and the chooser after a drawn game rest on unconfirmed suggestions | runner | port-preserve + reporting | unit + e2e | resolved |
| W-110 | Robustness fuzzing uses only random agents | tooling | tooling | e2e | resolved |
| W-111 | Coverage gates count tags, not behaviour | tooling | tooling | unit | resolved |
| W-112 | The long slow suite is not deselected by default | tooling | tooling | e2e | resolved |
| W-113 | Test helpers stop silently at step caps | tooling | tooling | unit | resolved |
| W-114 | The documented-command checker: shell blocks only, a network-dependent skip, timeouts that leave children running | tooling | tooling | unit | resolved |
| W-115 | Research artifacts are pinned to an old snapshot and outside the gate; regeneration is manual | tooling | tooling | unit | resolved |
| W-116 | Research scripts disagree with the engine on canonical printing and reminder-text stripping | tooling | port-preserve + test | unit | resolved |
| W-117 | The adversarial review was a sample | tooling | tooling | e2e | resolved |
| W-118 | verify_all.sh gate limits | tooling | tooling | e2e | resolved |
| W-119 | blind_transcripts.py uses a private helper and path-dependent ids, untested | tooling | tooling | unit | resolved |
| W-120 | Project Claude permissions are broader than the "only the refresh skill uses the network" rule | tooling | configuration change (needs the user's approval) | unit | open (user decision) |
| W-121 | Refresh tooling: only the offline core is tested; official fetches need a TLS workaround | tooling | tooling | unit | resolved |
| W-122 | Untracked bytecode of an unmerged later branch (bench/, tests/bench, tests/perf) | tooling | tooling (inventory) | unit | resolved |
| W-123 | compare.py usability limits | tooling | eval tooling | unit + e2e | resolved |
| W-124 | Search is engine-bound; invalidating the derived view by mutation kind is deferred | performance | Rust fix (output-identical) | e2e + unit | resolved |
| W-125 | Action and Decision are frozen dataclasses (slow construction on the hot path) | performance | Rust fix (by construction) | unit | resolved |
| W-126 | Throughput measurement counts wall time x workers on one pairing; machine state is not recorded | performance | tooling | e2e | open (run pending) |
| W-127 | Investigation evidence is not reproducible: raw outputs were gitignored and cited commits were squashed away | docs | docs + eval tooling | unit | resolved |
| W-128 | Stale counts across status.json and the research docs | docs | docs (generated counts) | unit | resolved |
| W-129 | Hand-maintained docs drift from the code | docs | docs (checked contracts) | unit | resolved |
| W-130 | The shuffle-cut and chooser-procedure wording disagree with the engine and REPORTS | docs | port-preserve + docs | unit | resolved |
| W-131 | Multiplayer (rules section 12) is out of scope | design-decision | admissible exception | n/a | admissible exception |
| W-132 | No sideboarding in BO3 | design-decision | admissible exception | n/a | admissible exception |
| W-133 | No clock: time limits, extra turns and tiebreaks are not modelled | design-decision | admissible exception | n/a | admissible exception |
| W-134 | Both players know both exact decklists | design-decision | AI (strengthened search, opt-in information mode) | unit + e2e | resolved (not adopted) |
| W-135 | Information-set honesty is a convention, not enforced by the API | design-decision | Rust fix (by construction) | unit + compile-fail | resolved |
| W-136 | SearchConfig defaults reproduce the pre-change AI; baseline presets live in code | design-decision | Rust fix (separate types) | unit | resolved |
| W-137 | The determinism contract; cross-platform identity depends on the C math library | design-decision | port-preserve + tests + portable math (strengthened search); the `erfc` FFI needs the lead's decision | unit + e2e | open (Linux run after the push) |
| W-138 | Multi-card choices are sequential single picks | design-decision | port-preserve + AI (strengthened search) | unit + ablation | resolved (not adopted) |
| W-139 | Opponent nodes are tuned against the searcher's true private cards | ai-search | AI (strengthened search) | scenario + ablation | resolved (not adopted) |
| W-140 | In-tree priors come from the first world that reaches each node | ai-search | AI (strengthened search) | unit + ablation | resolved (not adopted) |
| W-141 | The no-effect branch check compares a branch with a policy line that may replay the move | ai-search | AI (strengthened search) | unit + ablation | resolved (not adopted) |
| W-142 | The evaluation is fitted on true-state features but scores determinized states; Burst density is computed once per search | ai-evaluation | AI (strengthened search) + eval tooling | unit + ablation | resolved (not adopted) |
| W-143 | Battle-damage redirect cycles recurse without bound and crash the game (two GD04-095) | engine | Rust fix (error path only) + data tripwire | unit + e2e + negative control | resolved |
| W-144 | The deck parser accepts unbounded counts: a huge count crashes with a traceback or allocates without limit | validator | Rust fix (diagnostics) | unit | resolved |
| W-145 | The packaged card data predates GD06 (released 2026-10-30), so its cards are unknown | data | Rust fix (data-driven loader) + reporting | unit + e2e | resolved |
| W-146 | The rules markdown is checked against the official PDF only by its set of rule ids | data | data tripwire | unit + negative control | resolved |
| W-147 | The mulligan-outcome hypothesis is confounded by the AI's own redraw rule | reporting | Rust fix (additive analysis) | unit | resolved |
| W-148 | Saved replays are median-length games, and the CLI cannot replay other games.ndjson records | reporting | Rust fix (opt-in) | e2e | resolved |
| W-149 | Which failure a parallel run reports depends on completion timing | runner | Rust fix (error path only) | unit | resolved |
| W-150 | Determinism acceptance tests run only with the random test agent | tooling | tooling | e2e + negative control | resolved |
| W-151 | Fuzz coverage counts cards in decks, not cards executed; tokens and EX cards are never in decks; the seed is fixed | tooling | tooling | unit + e2e | resolved |
| W-152 | Seat and first-player symmetry was checked once and has no regression test | tooling | tooling | e2e + negative control | resolved |
| W-153 | News and errata discovery in the refresh skill reads only a few listing pages and flags entries by title | tooling | tooling (Python side, handed to the lead) | unit | resolved |
| W-154 | Untracked port artifacts would break the project's Python gates once committed | tooling | tooling | e2e | resolved |
| W-155 | Change detection ignores edits inside stripped parentheticals, so a reworded semantic parenthetical passes the refresh diff, the golden check and coverage | tooling | data tripwire + tooling (Python side, handed to the lead) | unit + negative control | resolved |
| W-156 | Replay files and games.ndjson records carry no data, rules or code version | reporting | Rust fix (diagnostics, output bytes unchanged) | unit + e2e | resolved |
| W-157 | The card database silently loads with no overrides when overrides.json is missing | data | Rust fix (fail fast) | unit | resolved |
| W-158 | A damage-prevention threshold that is not a literal integer is silently read as unlimited prevention | engine | Rust fix (fail at registry build) + data tripwire | unit + negative control | resolved |
| W-159 | Turn-history conditions match their filters against a card's current state, not its state when the event happened | engine | port-preserve + data tripwire | unit + negative control | resolved |
| W-160 | The Python-vs-Rust equivalence harness compares results.json only up to JSON formatting, skips decision logs and the fast preset, and can reuse a stale Python run | tooling | tooling | unit + negative control | resolved |
| W-161 | ToDeck can offer duplicate ARRANGE options when a targeted Unit's Pilot is also a target | engine | port-preserve + data tripwire | unit | resolved |
| W-162 | Trigger and frame fields that are serialised but never read (host_seq, optional, answer, Battle.blocked) may hide last-known-information bugs | engine | port-preserve + fuzz diagnostic | unit + fuzz | resolved |
| W-163 | The Burst acceptance score assumes that no Burst should be declined, which was checked only on the example and test decks | ai-policy | AI (strengthened search) + data tripwire | unit + negative control | resolved |
| W-164 | Token-definition fallbacks and two override-loader forms are never exercised by current data | data | Rust fix (implemented with synthetic-data tests) + data tripwire | unit + negative control | resolved |

## AI search

### W-01 — Two-turn rollout horizon: plans that pay off later rest on the evaluation
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:111-115`, `docs/python/AI.md:443-452`, `docs/python/AI.md:366-368`, `docs/python/AI.md:509`, `src/gcg_sim/ai/config.py:30`, `src/gcg_sim/ai/mcts.py:119`, `README.md:167-169`, `.claude/skills/gcg-benchmark/SKILL.md:114-117`, `bench/__pycache__/weakness.cpython-315.pyc`, `81a45d9`, `23ddd49`
- **Quotes:** “Rollouts stop at the start of the turn after the reply turn; plans that pay off later rely on the evaluation.”; “A three-turn horizon costs about 43% more per iteration and is deferred for `strong`.”; “Decks that need long, precise lines may be underrated.”; “horizon_turns: int = 2”
- **What is weak:** rollouts cover the rest of the current turn plus the opponent's reply turn and stop at the next turn's first main-phase decision; anything that pays off later is priced only by the linear evaluation (the calibrated `value_scale` and the role can-kill/race terms partly compensate). Measured: a three-turn horizon costs about 43% more per iteration; on the older engine a three-turn horizon at 2x budget won 51/96. The deferred proposal is "a three-turn horizon for `strong`"; the README warns that decks needing long, precise lines may be underrated (category WK8 "long precise lines" of the lost weakness benchmark).
- **Resolution plan:** in the strengthened search only (the reference keeps `horizon_turns = 2`): (1) horizon as a preset parameter, with `standard` (budget from reference `strong`) at 3 turn boundaries and `fast` at 2; rollout decisions are counted in the node budget so the extra cost is explicit; (2) adaptive horizon: an iteration whose horizon leaf is "unsettled" (either side's race estimate at most 2 turns, or a pending battle/effect) plays one more turn boundary, decided only from the determinized world so it stays information-set honest; (3) the leaf constants generalised as in W-02; (4) the evaluation refitted at the new leaf point (W-34), because leaves then sit one turn later.
- **Verification** (scenario + ablation): scenario "two-turn plan": testkit position where the only winning line deploys a Link Unit and keeps a Blocker this turn so that a lethal attack exists on the searcher's turn after next (beyond the two-boundary horizon), while an immediate trade looks better at the reference's leaf; 16 agent seeds, reference `standard` must fail at least 8 (recorded), strengthened `standard` must solve at least 14. Ablation rows: horizon 3 alone, adaptive extension alone, both, and both with the refit evaluation; each reports nodes per game.
- **Status:** resolved: `adaptive_horizon` is adopted in both shipped presets (M15). A fixed `horizon_turns = 3` (or 4) measured negative or null at both budgets and is not adopted. This deviates from the plan's "`standard` starts at 3", and the measurement decides it (brief step 22). The evaluation refit at the new leaf point (plan item 4) is W-34's; it measured negative and is not adopted. Records: `docs/port/eval/weakness-status/P6-A.md`, `docs/port/eval/weakness-status/M15-bundle.md`.
- **Evidence:** scenario test W-01 (T-107; `crates/gcg-search/tests/scenarios/p6a_w01_two_turn_plan.rs`, 3 tests, pass). At the reference `standard` budget, the reference plays a winning plan in 0/16 seeds (it trades in all 16; recorded, bar ≤ 8). Horizon 3 plays one in 16/16, `adaptive_horizon` in 16/16 and the P6-A bundle in 16/16 (bar ≥ 14). At the reference `strong` budget the reference plays one in 16/16, so the position does not reproduce the weakness there, and the P6-A bundle in 14/16 (bar ≥ 14). Unit tests in `crates/gcg-search/src/mech/p6a/tests.rs` pass: `the_unsettled_test_reads_the_race_and_pending_resolutions`, `adaptive_horizon_extends_an_unsettled_leaf_once` and `adaptive_horizon_plays_longer_lines_deterministically`. The extension adds about 35% applies per game at the same iterations.
- **Ablation:**
  - Individual rows (reference `standard` budget, dev 20100100, `docs/port/eval/P6-A/rows/`):
    - `adaptive`: 218/400 (0.545, [0.496, 0.593]), +0.090 [+0.018, +0.161], discordant 37/19, 10010 vs 9864 / 160122 vs 116450, vs placebo +0.0250 [-0.0219, +0.0719] (`dev-individual-adaptive-400.json`).
    - `adaptive` confirmation (confirm 30100100, run once): 830/1600 (0.519, [0.494, 0.543]), +0.037 [+0.000, +0.075], 132/102, 9954 vs 9951 / 159206 vs 117625, vs placebo +0.0194 [-0.0042, +0.0430] (`confirm-individual-adaptive-1600.json`).
    - `horizon3`: 185/400 (0.463, [0.414, 0.511]), -0.075 [-0.148, -0.000], 22/37, 9755 vs 9932 / 174528 vs 116352.
    - `horizon4`: 201/400 (0.502, [0.454, 0.551]), +0.005 [-0.067, +0.077], 28/27, 9963 vs 10064 / 228382 vs 119610.
    - `p6a-h3`: 194/400 (0.485, [0.436, 0.534]), -0.030 [-0.098, +0.039], 22/28.
    - `p6a-h2` confirmation: 834/1600 (0.521, [0.497, 0.546]), +0.043 [+0.006, +0.079], 128/94.
  - High-budget rows (dev 20100200):
    - `adaptive`: 203/400 (0.507, [0.459, 0.556]), +0.015 [-0.057, +0.087], 29/26, 32675 vs 32755 / 506201 vs 373842.
    - `horizon3`: 194/400 (0.485, [0.436, 0.534]), -0.030 [-0.111, +0.051].
    - `horizon4`: 201/400 (0.502), +0.005 [-0.067, +0.077], at 1.9x the applies.
  - Bundle: `B_core` confirmed +0.068 [+0.030, +0.105] (see "Phase 6 outcomes").
  - Leave-one-out (dev 21500200): 203/400 (0.507, [0.459, 0.556]), +0.015 [-0.058, +0.088], discordant 30/27, 31734 vs 32752 / 579343 vs 407670, vs placebo -0.0100 [-0.0580, +0.0380] (`docs/port/eval/M15/rows/dev-loo-adaptive_horizon-400.json`). Removing it does not help, so it is kept. It costs about 38% of the bundle's applies.

### W-02 — Leaf-value constants cap the horizon at four turns
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/config.py:9-10`, `src/gcg_sim/ai/config.py:48-53`, `src/gcg_sim/ai/mcts.py:254-264`, `docs/python/AI.md:116-119`, `docs/PROGRESS.md:158`, `d9d41af`
- **Quotes:** “horizon_turns={self.horizon_turns} is too long”; “SearchConfig rejects a horizon at which a win at the horizon would not outrank every evaluated position (WIN_DECAY * horizon_turns < LEAF_FLOOR)”
- **What is weak:** `WIN_DECAY = 0.01` and `LEAF_FLOOR = 0.05` force `WIN_DECAY * horizon_turns < LEAF_FLOOR`, so at most 4 turns; terminal values are shaded 0.01 per turn. Any deeper horizon or extension (W-01, W-03) must re-derive the constants or a certain win stops outranking every estimate.
- **Resolution plan:** the strengthened search takes the leaf mapping as parameters: estimate `floor + (1 - 2 floor)·σ(score/scale)` and terminal value `1 - decay·turns`, with `decay = floor / (max_horizon + max_extension + 1)` computed per preset, and keeps the invariant as a constructor check. The reference keeps the Python constants and its check unchanged.
- **Verification** (unit): property test over horizons 1..8 and extensions 0..2: for every strengthened config, the worst terminal win (longest line) exceeds the best possible estimate and the best terminal loss is below the worst estimate; reference config with `horizon_turns = 5` is still rejected with the Python message.
- **Status:** resolved by measurement (not adopted): `generalized_leaf` is implemented and unit-tested. It measured null to slightly negative at `horizon_turns = 2`, so it is off in both shipped presets. It stays available and is needed whenever the longest line passes four turns, where the reference constants are refused. The reference keeps `LEAF_FLOOR`, `WIN_DECAY` and its check (`docs/port/eval/weakness-status/P6-A.md`).
- **Evidence:** the unit tests in `crates/gcg-search/src/mech/p6a/tests.rs` pass:
  - `generalized_leaf_constants_order_terminal_results_at_every_horizon`: the plan's property test over horizons 1..8 and extensions 0..2.
  - `the_reference_constants_keep_their_check`: 4 turns pass, 5 fail, and the reference configuration with `horizon_turns = 5` is still rejected with the Python message.
  - `the_leaf_check_covers_the_adaptive_extension`, `a_search_refuses_a_horizon_its_leaf_constants_cannot_order` and `leaf_values_use_the_configured_constants`.
- **Ablation:**
  - Individual (dev 20100100): 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.036, +0.105], discordant 30/23, 9979 vs 9920 / 120640 vs 119940, vs placebo -0.0025 [-0.0202, +0.0152] (`docs/port/eval/P6-A/rows/dev-individual-generalized-400.json`).
  - Confirmation (confirm 30100100): 789/1600 (0.493, [0.469, 0.518]), -0.014 [-0.048, +0.021], 95/106, 9779 vs 9865 / 118095 vs 118781, vs placebo -0.0063 [-0.0122, -0.0003].
  - High budget (dev 20100200): 195/400 (0.487, [0.439, 0.536]), -0.025 [-0.095, +0.046], 24/29, 32835 vs 32954 / 381563 vs 382522.
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-03 — Lethal-threat defence: the audited "keep the last Blocker" position is unsolved; threat-aware rollouts were measured null and removed
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:443-452`, `docs/PROGRESS.md:146-149`, `docs/ASSUMPTIONS.md:92`, `tests/ai/test_puzzles.py:5`, `bench/__pycache__/weakness.cpython-315.pyc`, `d9d41af`
- **Quotes:** “it won 449/800 against 448/800 without it on the same deals, and on 100 decisions where it changed the move the old move did better in play-outs (36 to 23), so it was removed”; “with the Blocker guard and the lethal extension the AI keeps its last Blocker (8/8 seeds; baseline and guard alone trade it 8/8)”; “Remove the threat-aware rollout settings (lethal extension, face-first push, Blocker guard): measured, they do not help.”
- **What is weak:** `lethal_extension_turns` (triggers can_kill/threat/opponent), `rollout_push` (face attacks score LETHAL/2) and `rollout_guard` were built and measured: 449/800 vs 448/800 (seed 36004, 56:55 discordant), changed decisions favoured the old move 36:23 (p = 0.12). They fixed one audited defence position (blue-white-federation vs purple-white-tekkadan, match 1, decision 93) where the baseline trades its last Blocker 8/8; that puzzle left the tree with the settings, so the shipped presets presumably still trade it (WK2 "lethal avoidance").
- **Resolution plan:** do not reinstate rollout-wide threat settings. Strengthened search adds a threat-aware leaf: when the horizon leaf is the opponent's turn start (or its next attack step), an exact bounded solver of the opponent's attack step on the determinized board (engine-correct eligible Blockers, High-Maneuver, Shields and Base HP, AP per attacker, CANT_ATTACK; the opponent's hand from the determinized world) replaces the estimate with the time-shaped loss value when the opponent has lethal in that world. Solver work is counted in nodes. The same solver backs W-17 and W-38.
- **Verification** (scenario + ablation): rebuild the defence puzzle from the audit seeds (90031–90033, five BO1 games per example pairing): regenerate the game deterministically, freeze decision 93 of the bwf-pwt match 1 as a Rust scenario (or rebuild it with the testkit as d9d41af did); 8 agent seeds, reference `standard` trades the Blocker 8/8 (recorded), strengthened `standard` keeps it at least 7/8. Ablation rows: threat-aware leaf alone and leave-one-out from the bundle; adoption needs more than the null 449/448 result.
- **Status:** resolved: `tactical_solver` is adopted in both shipped presets (M15). `threat_leaf` measured negative and is not adopted. No rollout-wide threat settings were reinstated. The audited position could not be regenerated: its command and seed mapping are lost (W-17), so the scenario rebuilds its shape (`docs/port/eval/weakness-status/P6-B.md`).
- **Evidence:** scenario test W-03 (T-108; `crates/gcg-search/tests/scenarios/p6b_w03_last_blocker.rs`, pass) ran 8 agent seeds at the reference `standard` budget:
  - The last Blocker is kept 0/8 by the reference (it trades it 8/8, pinned), 0/8 by the null configuration and 8/8 by `tactical_solver` (bar ≥ 7/8).
  - Root values at seed 1: the null configuration rates the attack 0.885 and ending the turn 0.829; with `tactical_solver` the attack is 0.010 and ending the turn 0.834.
  - Unit tests `forced_win_reads_the_defenders_blocks` and `a_leaf_with_a_proved_lethal_takes_the_time_shaped_terminal_value` (`crates/gcg-search/src/mech/p6b/tests.rs`) pass.
- **Ablation:**
  - `tactical_solver` individual (dev 20200100): 212/400 (0.530, [0.481, 0.578]), +0.060 [-0.007, +0.126], discordant 30/18, 9934 vs 9740 / 131226 vs 116345, vs placebo +0.0025 [-0.0200, +0.0250] (`docs/port/eval/P6-B/rows/dev-tactical_solver-400.json`).
  - `tactical_solver` high budget (dev 20200200): 213/400 (0.532, [0.483, 0.581]), +0.065 [-0.007, +0.136], 34/21, 32178 vs 32431 / 404687 vs 375681.
  - `tactical_solver` confirmation (confirm 30200200, run once): 811/1600 (0.507, [0.482, 0.531]), +0.014 [-0.023, +0.050], 116/105, 32202 vs 32208 / 412290 vs 375393 (`confirm-hb-tactical_solver-1600.json`).
  - `threat_leaf` individual: 205/400 (0.512, [0.464, 0.561]), +0.025 [-0.044, +0.094], 28/23, 9659 vs 9649 / 129095 vs 116530, vs placebo -0.0150 [-0.0418, +0.0118].
  - `threat_leaf` high budget: 197/400 (0.492, [0.444, 0.541]), -0.015 [-0.092, +0.062], 30/33, 32062 vs 32731 / 421183 vs 381468.
  - Leave-one-out of `tactical_solver` (dev 21500200): 215/400 (0.537, [0.488, 0.586]), +0.075 [+0.009, +0.140], discordant 31/16, 32808 vs 32150 / 580619 vs 476924, vs placebo +0.0200 [-0.0132, +0.0532] (`docs/port/eval/M15/rows/dev-loo-tactical_solver-400.json`). The solver costs 109,470 auxiliary applies per game, 19% of the bundle's.

### W-04 — Budget plateau: strength is model-limited past about 2x budget; `strong` is barely stronger than `standard`
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/python/AI.md:329-343`, `docs/python/AI.md:358-368`, `docs/python/AI.md:479-485`, `docs/python/AI.md:509`, `docs/PROGRESS.md:104-106`, `docs/PROGRESS.md:110-112`, `docs/status.json:41`, `src/gcg_sim/ai/config.py:98-109`, `d9d41af`, `81a45d9`
- **Quotes:** “`strong` keeps twice `standard`'s and is only slightly stronger (0.523 above).”; “every root rule tried (sequential halving, progressive widening, near-tie extensions, visit minimums, budget-dependent exploration) was null within ±0.0004 at equal iterations.”; “209/400 = 0.523 [0.474, 0.571] (pre-change 230/400 = 0.575)”; “new standard 472/800 (0.590, [0.556, 0.624]); the same settings at the old budget 435/800 (0.544); the old AI at twice the budget 433/800 (0.541)”
- **What is weak:** decision regret against a much larger search is 0.0035 at half budget, 0.0019 at the old budget, 0.0013 at 2x and 0.0006 at 4x; past 2x the limit is rollouts, horizon and evaluation, not root allocation. `strong` vs `standard` 209/400 (before the change 230/400); the old knobs at 4x iterations scored 46/96. Better search and more iterations each gave about 4 points (the presets doubled `standard`'s iterations and traded throughput for it). Deferred: "a larger `strong` budget relative to the new `standard`".
- **Resolution plan:** strengthened `standard` starts from reference `strong` budgets and spends nodes on the model, not the root rule: the horizon (W-01), threat-aware leaf (W-03), evaluation refit (W-34, W-37), calibrated policy (W-23) and tree reuse/transpositions (W-11). Add root-parallel deterministic search: K independent trees with seeds `derive_seed(seed, "tree", k)` over disjoint world sets, merged by summed visits in fixed tree order, so the result is identical for any thread count; K and the per-tree budget are chosen on the budget curve. gcg-eval `budget-curve` measures strength at 0.5x, 1x, 2x and 4x of the starting budget to pick each preset's node budget at the knee.
- **Verification** (ablation + unit): ablation rows: budget curve of the strengthened search vs reference `strong` (at equal nodes and at 2x), root-parallel K = 1/4/8 at equal total nodes, and the full bundle; acceptance: strengthened `standard` beats reference `strong` with Wilson lower bound above 0.5 on a held-out seed run once. Unit test: root-parallel decision and statistics are bit-identical with 1, 4 and 10 threads.
- **Status:** resolved: the verification's acceptance holds. In final evaluation 2 (`docs/port/eval/final-2/README.md`, M15b presets, eval master 90300001, run once) strengthened `standard` beat reference `strong` 6408/11200 (0.572, [0.563, 0.581]) over the 28 panel pairs and 2920/5200 (0.562, [0.548, 0.575]) over the 13 held-out pairs, both lower bounds above 0.5. (The plan's stricter A26 gate, a held-out lower bound of at least 0.55, misses by 0.002; `docs/port/eval/notes.md`.) The first final evaluation (`docs/port/eval/final-1/README.md`, M15 presets) was not accepted. The mechanism part is measured: `root_trees{K}` is resolved by measurement and not adopted. It is null to negative at equal iterations and shifts the budget curve down rather than moving the plateau. The shipped `standard` spends its budget on 8x the reference `strong` iterations with `B_core` (`docs/port/eval/M15/README.md`, `docs/port/eval/weakness-status/P6-D.md`).
- **Evidence:**
  - Unit and scenario tests (`crates/gcg-search/tests/scenarios/p6d_determinism.rs`, pass):
    - `root_trees_give_the_same_decision_for_any_worker_count`: K = 4, K = 8 and the P6-D bundle on pools of 1, 4 and 10 workers and inline give an identical action, alternatives, statistics, counters and generator state.
    - `root_trees_split_the_budget_and_draw_one_seed` and `the_bundle_repeats_in_another_process`.
    - `mech::p6d::ensemble::tests` (4 tests).
  - Budget curve of the M15 bundle against `ref:strong` (tuning 11500100): 1x 195/400 (0.487), 2x 205/400 (0.512), 4x 214/400 (0.535), 8x 227/400 (0.568, [0.518, 0.615]). At 8x the paired difference is +0.135 [+0.059, +0.208], discordant 45/18, 122934 vs 16891 / 2197523 vs 197816 (`docs/port/eval/M15/curves/tuning-standard-bundle-vs-ref-strong.json`).
  - The same budgets without the bundle: 2x 212/400 (0.530), 8x 215/400 (0.537) (`docs/port/eval/M15/curves/tuning-standard-nobundle-vs-ref-strong.json`).
  - Sanity row of the shipped `standard` against `ref:strong` (dev 21500400, 420 games): 246/420 (0.586, [0.538, 0.632]), +0.171 [+0.097, +0.243], 52/16, 118573 vs 16604 / 2070098 vs 191480 (`docs/port/eval/M15/rows/dev-sanity-standard-vs-ref-strong-420.json`).
  - M15b `standard` against `ref:strong` (tuning 10500001, 1,620 games): 950/1620 (0.586, [0.562, 0.610]), +0.173 [+0.136, +0.208], discordant 194/54, 127010 vs 16553 / 2253635 vs 193145, no pair with an upper bound below 0.5 (`docs/port/eval/M15b/rows/tuning-std-b-vs-ref-strong-1620.json`).
  - E1 of the first final evaluation (M15 presets): 6268/11200 (0.5596, [0.5504, 0.5688]); held-out pairs 2885/5200 (0.5548, [0.5413, 0.5683]), no held-out pair with an upper bound below 0.5 (`docs/port/eval/final-1/README.md`). Both lower bounds are above this entry's bar of 0.5, but that evaluation was not accepted as a whole, and M15b changed the presets after it.
- **Ablation:**
  - Individual rows at equal total iterations (reference `standard` budget, dev 24600001, `docs/port/eval/P6-D/individual-root_trees-*.json`):
    - control (null configuration): 201/400 (0.502, [0.454, 0.551]), +0.005 [-0.059, +0.069], discordant 22/21.
    - `root_trees=1`: 201/400, +0.005 [-0.063, +0.073], 25/24, 9800.2 vs 9965.3 / 118321.3 vs 120362.2.
    - `root_trees=4`: 190/400 (0.475, [0.426, 0.524]), -0.050 [-0.117, +0.017], 19/29, 10268.6 vs 9888.8 / 123025.3 vs 117290.6.
    - `root_trees=8`: 178/400 (0.445, [0.397, 0.494]), -0.110 [-0.189, -0.029], 24/46, 10358.9 vs 9813.6 / 123188.3 vs 114461.3.
  - Budget curves against `ref:standard` (dev 24600002) at 0.5x, 1x, 2x and 4x: one tree 0.502, 0.502, 0.510, 0.545; `root_trees=4` 0.435, 0.445, 0.500, 0.510 (`docs/port/eval/P6-D/budget-curve-null-standard*.json`).
  - Leave-one-out: not applicable (not in the shipped bundle).
  - E1: final evaluation 2 pending (`docs/port/eval/final-2/`).

### W-05 — Strategy fusion: opponent nodes pool statistics across hidden worlds (single-observer ISMCTS)
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:105-110`, `docs/python/AI.md:462-469`, `docs/python/AI.md:512-513`, `src/gcg_sim/ai/mcts.py:1-18`, `src/gcg_sim/ai/mcts.py:233-245`, `d9d41af`
- **Quotes:** “Opponent nodes pool statistics across worlds where the opponent holds different cards (0.84–0.89 distinct hands per visit)”; “Information-set MCTS with determinizations (single-observer ISMCTS).”; “auto-pass splits on 183 census positions: 389 in 24 positions, now 0”
- **What is weak:** one tree from the decider's view; `observation_keys` removed the auto-pass splits (389 in 24 of 183 positions to 0) but 12 splits of other causes remain; opponent nodes average over worlds in which the opponent holds different cards (0.84–0.89 distinct hands per visit). Measured fusion effect at opponent nodes: +0.021 at a large budget. A multiple-observer search is deferred because opponent nodes would get almost no visits at these budgets.
- **Resolution plan:** strengthened search (budgets of reference `strong` and up): partial multiple-observer opponent nodes. Opponent node statistics are split by a coarse, deterministic abstraction of the opponent's determinized private state (at most 4 buckets: holds a castable action-step trick or not, holds a Blocker-relevant card or not), so the opponent's reply depends on what it could hold without starving visits; a bucket's own statistics are used for selection only once it has at least `m` visits, and below that the node's pooled statistics are used. `m` is a preset parameter. It starts at 8 and is fixed once by a sweep over {4, 8, 16, 32} on development seeds: one ablation row per value, the largest paired difference wins, and ties go to the smaller value. The sweep rows and the chosen `m` are recorded. Combined with belief weighting (W-07). The 12 remaining splits are enumerated by a census tool in gcg-eval (the port of the 183-position census). Each split gets an observation key that removes it. If no key can be built, the split needs a decision-level ablation with the W-43 regret tool instead: with and without the split, on every census position where it occurs. The 95% interval of the regret difference must lie within ±0.0004 per decision, the bar at which the root rules of W-04 were called null. A split that meets neither stays open in this entry.
- **Verification** (scenario + ablation): scenario: the opponent holds an AP−3 action-step Command in half of the worlds; attacking into it loses the attacker, holding back is correct; 16 seeds, reference `standard` attacks in at least 8 (recorded), strengthened avoids it in at least 13. Ablation rows: bucketed opponent nodes alone and leave-one-out, plus the `m` sweep. The census reports, for each of the 12 splits, the observation key that removes it or its no-effect ablation interval, and 0 splits with neither. The Rust port of `tests/ai/test_information_set.py` (permutation invariance plus the negative control) must pass for both strengthened presets.
- **Status:** resolved by measurement (not adopted): `opponent_buckets{m}` is implemented, swept (m = 4 chosen) and measured null at both budgets, so it is not adopted. By the lead's decision (`docs/port/PLAN.md` §0.1, end of Phase 6) GAP-A1's measured break-even trick share (0.497) replaces the scenario bar (`docs/port/eval/weakness-status/GAP-A1.md`).
  1. The census half of the verification is done (GAP-A1). `gcg-eval weakness census` counts every observation-key split of the searcher's tree on 180 positions, by cause. The auto-pass splits behave as Python measured them: 237 with `observation_keys` off, 0 with them on. Every remaining split has an observation key that removes it by construction: `transpositions` keys every node by the searcher's observation. That key is measured null (W-11). So 0 splits have neither a key nor a regret interval.
  2. The scenario bar is missed, and GAP-A1 measured why. The bar asks that the strengthened search avoids the attack in at least 13 of 16 seeds. The reference and `opponent_buckets=4` and `=8` all attack in 16/16. Under `ROLE_WEIGHTS` the two moves tie at a trick share of 0.497, so with the trick in half of the worlds they differ by 0.0002 in the search's own evaluation, and no setting of the search makes 13 of 16 a reliable result. The lead accepted this measurement in place of the bar. A leaf evaluation that prices the bounce would be the way to change the decision; the W-34 and W-35 refits measured null or negative.
- **Evidence:**
  - The census (`docs/port/eval/GAP-A1/weakness/census-report.json`; 180 positions from 60 reference `standard` self-play games, dev 22710500): reference `standard` 1,943 splits in 90 positions; with `observation_keys` off 2,190, of them 237 `opponent_pass_vs_no_decision`; shipped `fast` 3,316; shipped `standard` 27,638 (M15 presets). `weakness_suite::tests::the_census_counts_splits_by_cause` passes.
  - Scenario values (`crates/gcg-search/tests/scenarios/gapa1_w05_fusion_values.rs::w05_the_evaluation_prefers_the_attack_below_a_high_trick_share`, pass), 4,000 iterations: trick hidden, attack 0.3163 against ending the main phase 0.2939; trick revealed 0.3134 against 0.3349; filler revealed 0.2878 against 0.2666.
  - Unit test `mech::p6c::tests::w05_bucketed_opponent_nodes_reply_by_hand` uses the fusion position with two seeds and 2,000 iterations. The table counts Block visits among the block decision's visits:

    | Selection | No trick in the world | Trick in the world |
    | --- | --- | --- |
    | bucketed, `m = 4` | 50/724 and 60/725 | 256/276 and 253/275 |
    | pooled | 267/724 and 129/725 | 110/276 and 36/275 |

  - Scenario test W-05 (T-109; `crates/gcg-search/tests/scenarios/p6c_w05_fusion.rs::w05_fusion_decisions_are_recorded`) passes as a record. Attacks of 16 seeds at the reference `standard` budget: reference 16, `opponent_buckets=4` 16, `=8` 16.
  - `every_flag_runs_on_the_vector_decisions_and_reads_the_information_set_only` shows permutation invariance on 32 vector decisions.
  - The strengthened search's information-set port, `crates/gcg-search/tests/observation.rs::the_strengthened_decision_is_invariant_under_hidden_permutation`, passes (a `search-fast` agent).
- **Ablation:**
  - Individual rows (reference `standard` budget, dev 20300001, `docs/port/eval/P6-C/individual-*.json`), with iterations and applies per game challenger / opponent:
    - `opponent_buckets=4`: 203/400 (0.507, [0.459, 0.556]), +0.015 [-0.056, +0.086], discordant 28/25, 9937.2 / 9887.2, 119556.8 / 118690.0.
    - `=8`: 203/400 (0.507), +0.015 [-0.056, +0.086], 28/25, 9914.9 / 9888.2, 120492.1 / 119423.3.
    - `=16`: 198/400 (0.495, [0.446, 0.544]), -0.010 [-0.083, +0.063], 27/29.
    - `=32`: 196/400 (0.490, [0.441, 0.539]), -0.020 [-0.091, +0.051], 25/29.
    - control (`engine_error_leaf=false`): 199/400 (0.497), -0.005 [-0.077, +0.067], 27/28.
  - High budget, `opponent_buckets=4` (dev 20300002): 196/400 (0.490, [0.441, 0.539]), -0.020 [-0.094, +0.054], 27/31, 33066.0 / 33107.2, 391066.4 / 390722.6 (`high-budget-opponent_buckets-4.json`).
  - Not confirmed: no row reached +0.03.
  - The census's key, `transpositions`, has P6-D's rows (W-11): individual +0.020 [-0.051, +0.091] (dev 24600001), confirmation -0.006 [-0.042, +0.029] (confirm 30460002), and it was in `B_mid`. The census and the value analysis change no search, so they have no row of their own.
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-06 — Observation keys count one child twice when the opponent passes twice in a row
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:469-474`, `src/gcg_sim/ai/mcts.py:1-18`, `src/gcg_sim/ai/mcts.py:233-245`, `d9d41af`
- **Quotes:** “both passes map to one child, which that iteration then counts twice (9 of 11,402 iterations in instrumented `fast` self-play, all on the searcher's turn)”; “the exact condition of the observation-key double count”
- **What is weak:** when the opponent passes in a battle's action step and again in the end phase's action step of the same turn with no multi-move decision between them, both passes map to one child and that iteration updates it twice (9 of 11,402 iterations).
- **Resolution plan:** strengthened search backpropagates through a per-iteration visited set: each node is updated at most once per iteration. The reference keeps the Python behaviour (it must replay bit for bit).
- **Verification** (unit + ablation): unit test with a scripted line (battle action-step pass, end-phase pass, no multi-move decision between): the shared node's visit count grows by 1 per iteration in the strengthened search and by 2 in the reference (pinned). Included in the bundle's leave-one-out row.
- **Status:** resolved by measurement (not adopted): `visited_set_backup` backs a child up once per iteration: visits, value and the `avail` count. It measured null (-0.005), so it is off in both presets. The reference keeps the double count (pinned). The P6-D file notes it as a correctness fix the lead may still adopt on this evidence.
- **Evidence:** the tests in `mech::p6d::backup::tests` pass:
  - `a_child_reached_twice_gains_one_visit_per_iteration`: on a scripted line the shared child gains 1 visit per iteration with the visited set and 2 in the reference (pinned).
  - `a_path_without_repeats_backs_up_as_the_reference`: bit-identical to the reference.
  - `a_repeated_opponent_pass_is_backed_up_once_with_the_visited_set`: over 144 D2 vector decisions, the reference repeated a child 37 times, every one an opponent ACTION_STEP pass backed up twice. With the visited set, each of the 55 repeated children gained one visit.
- **Ablation:**
  - Individual (dev 24600001): 199/400 (0.497, [0.449, 0.546]), -0.005 [-0.072, +0.062], discordant 23/24, 10006.6 vs 10000.0 / 122330.8 vs 121798.0 (`docs/port/eval/P6-D/individual-visited_set_backup-true.json`).
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-07 — Determinization samples hidden cards uniformly: no inference from the opponent's actions
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/engine/observe.py:1-6`, `src/gcg_sim/engine/observe.py:56-83`, `src/gcg_sim/ai/evaluation.py:3-6`
- **Quotes:** “Everything else is drawn from the multiset of that owner's decklist minus”
- **What is weak:** hidden hands, Shields and deck order are drawn uniformly from the decklist minus known cards, whatever the opponent has done (passing with Resources open, holding cards, declining blocks or Bursts).
- **Resolution plan:** belief-weighted determinization for the strengthened search: each candidate world gets a weight equal to the likelihood of the opponent's last N public decisions under the opponent model (softmax of the policy scores at each recorded decision, replayed on the candidate world), and worlds are drawn by deterministic systematic resampling with the decision seed. Only public history and the searcher's own knowledge enter, so information-set honesty holds. The engine's `determinize` stays unchanged for the reference.
- **Verification** (scenario + ablation + unit): scenario: the opponent kept 2 Resources open and passed its main phase with 2 cards in hand, consistent with holding an action-step trick; 16 seeds, reference attacks into the trick in at least 8 (recorded), strengthened avoids it in at least 12. Unit tests: weights unchanged when a hidden card of the true state is changed (honesty), and permutation invariance as in W-05. Ablation row alone and leave-one-out.
- **Status:** resolved by measurement (not adopted): `belief_worlds{M}` with `belief_last_n{N}` is implemented and meets the scenario bar. It is null at game level: a +0.035 dev row at the high budget did not hold on 1,600 confirm games. It is off in both presets. `docs/port/eval/weakness-status/P6-C.md` records two deviations:
  - Only the opponent's last main-phase end is weighed. The agent sees no earlier decisions, and weighing more needs an `Agent::observe` hook (P6-C interface request 4).
  - The model is the reference playout policy's MAIN scores at `prior_temperature`, because P6-F's calibrated policy is not adopted.
- **Evidence:**
  - Scenario test W-07 (T-110; `crates/gcg-search/tests/scenarios/p6c_w07_open_resources.rs::w07_belief_worlds_hold_back_from_the_inferred_trick`, pass). Attacks of 16 seeds at the reference `standard` budget: reference 16, `belief_worlds` 8: 16, 16: 12, 32: 8, 64: 4. With 64 it holds back in 12.
  - Unit test `mech::p6c::tests::w07_belief_worlds_infer_the_trick_from_the_open_resources`: of 400 worlds, the trick is in the opponent's hand in 121 reference worlds, 178 at M = 4, 224 at M = 8 and 243 at M = 16.
  - Unit tests `the_end_of_the_main_phase_weighs_castable_hands_down`, `belief_worlds_are_candidates_of_the_world_seed`, `resampling_draws_by_cumulative_weight` and `settings_that_do_nothing_are_refused`.
  - The honesty and permutation test `every_flag_runs_on_the_vector_decisions_and_reads_the_information_set_only`.
- **Ablation:** iterations and applies per game are challenger / opponent.
  - Individual (dev 20300001):
    - `belief_worlds=4`: 203/400 (0.507, [0.459, 0.556]), +0.015 [-0.057, +0.087], discordant 29/26, 10027.5 / 9801.8, 126960.5 / 118237.0.
    - `=8`: 200/400 (0.500, [0.451, 0.549]), +0.000 [-0.070, +0.070], 26/26, 9872.2 / 9843.7, 130205.2 / 118792.8.
    - `=16`: 204/400 (0.510, [0.461, 0.559]), +0.020 [-0.053, +0.092], 30/26.
  - High budget (dev 20300002):
    - `=4`: 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.040, +0.109], 33/26, 32919.0 / 32804.7, 415918.2 / 388094.2.
    - `=16`: 204/400 (0.510), +0.020 [-0.053, +0.092], 30/26.
  - Confirmation of `=4` (confirm 30300001, run once): 790/1600 (0.494, [0.469, 0.518]), -0.013 [-0.044, +0.019], 80/90, 32029.9 / 32268.9, 399901.5 / 373145.0 (`docs/port/eval/P6-C/confirm-high-budget-belief_worlds-4.json`).
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-08 — Determinization pads or trims pools when effects move cards between players' decks
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + Rust fix (diagnostic)
- **Sources:** `src/gcg_sim/engine/observe.py:98-110`
- **Quotes:** “Pools match the slot count unless effects moved cards between players' decks; pad by”
- **What is weak:** `_fit` pads by repetition or trims deterministically, so a sampled world can hold card multisets the real game cannot contain; it only raises when the pool is empty.
- **Resolution plan:** the Rust engine keeps `_fit` bit-exact (the reference depends on it) and counts every pad/trim in a diagnostic counter. The strengthened search uses an exact pool: hidden-card multisets computed by conservation from both decklists plus every move the observer saw (owner and controller tracked separately), never padded; a conservation failure is an engine error.
- **Verification** (unit + fuzz): testkit scenario that moves a card into the other player's deck; assert the strengthened world's hidden multiset equals the true one and the reference pads (pinned). Fuzz invariant over 10,000 random games: per definition, the strengthened determinization's card counts equal the true state's.
- **Status:** resolved by measurement (not adopted): the engine keeps `_fit` bit-exact and counts every pad and trim (`observe::fit_adjustments`, read-only). `exact_pools` determinises every world with conservation-exact pools, never padded, and a conservation failure is an engine error (`observe::exact_pools`, `assign_hidden`, `determinize_exact_into`, `with_decklists`; `docs/port/eval/weakness-status/GAP-B.md`). It changes no game, so it is off in both presets. The engine places every card in its owner's zone list (`core::move_single`), so no rule puts a card into the other player's deck, and `_fit` pads or trims only in hand-built states.
- **Evidence:** these tests pass (GAP-B):
  - The testkit scenario, `crates/gcg-engine/tests/gapb_exact_pools.rs::w08_a_card_in_the_other_players_deck_is_padded_by_the_reference_and_exact_here`: a card of player 0 moved into player 1's deck. The reference pads and trims once per determinisation (pinned) and holds the wrong multisets. The exact world holds each owner's true hidden multiset.
  - The fuzz invariant, `gapb_exact_pools.rs::w08_fuzz_exact_worlds_keep_every_owners_hidden_card_counts`: 10,000 seeded random games, 71,507 sampled states, both observers. Every exact world's hidden card counts equal the true state's per owner, and `_fit` pads 0 and trims 0.
  - `mech::p6c::gapb_tests::w08_exact_pool_worlds_hold_the_true_multisets_where_the_reference_pads` (the search's worlds in the scenario: exact 64/64, reference 0/64) and `w08_exact_searches_are_the_reference_searches_on_the_vectors` (all 126 searched vector decisions equal the reference's).
- **Ablation:**
  - Individual (dev 22800001): 203/400 (0.507, [0.459, 0.556]), +0.015 [-0.061, +0.091], discordant 32/29, 9771 vs 9657 / 118823 vs 117258 (`docs/port/eval/GAP-B/individual-exact_pools-true.json`). Game by game against the placebo row of the same master (`individual-placebo-exact_pools-false.json`), 0 of 400 games changed.
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-09 — Move canonicalization never pays with an EX Resource on purpose
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:61-64`, `docs/python/AI.md:154`, `src/gcg_sim/ai/actions.py:41-45`, `src/gcg_sim/ai/actions.py:57-85`, `src/gcg_sim/ai/policy.py:42`, `docs/CONFLICTS.md:4894`, `docs/CONFLICTS.md:4916`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:249-272`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:24`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:52`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:100-113`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:115-125`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:446`, `bench/__pycache__/positions.cpython-315.pyc`
- **Quotes:** “the representative spends the fewest EX Resources, which are single-use”; “spend EX Resources only when needed”; “EX_PENALTY = 1.5”
- **What is weak:** `action_key` keeps only the play variant (`c // ALT_PLAY_BASE`) and `canonical` keeps the fewest-EX action, so plays differing only in EX count share one key and the search only sees the cheapest; rollouts also penalise EX use. Cards that reward EX payment can therefore never be played that way when a normal payment exists: GD04-020/GD04-021 (`ex_used >= 1`), GD04-085, GD04-106/GD04-108 (`PLAYED_WITH_EX`), GD05-118 (`wp_gd05_b_played_with_ex`) and the GD05-018 EX-exile reading. Not acknowledged in the docs; the lost benchmark had an `ex_variant` puzzle.
- **Resolution plan:** strengthened search derives an EX-sensitivity table from the effect IR at registry build (programs reading `EventAmount("ex_used")`, `EventFlag("ex_used")`, the `played_with_ex` conditions, EX-exile triggers, including friendly listeners such as GD04-085); for EX-sensitive plays the move key keeps one representative per distinct EX count, and the policy drops the EX penalty when the reward condition can be met. Other plays keep the fewest-EX representative. The reference is unchanged.
- **Verification** (scenario + unit + ablation): scenario with GD04-020 (draw when a Dawn of Fold Command is played using an EX Resource) where paying with EX is clearly better: reference never pays with EX (0/16, recorded), strengthened at least 14/16. Unit test: the derived EX-sensitive set contains every card whose IR reads those conditions (cross-checked against a scan of all programs). Ablation row (bundle only; few decks hold these cards).
- **Status:** resolved by measurement (not adopted): `ex_variants` keeps one root move per distinct EX count for EX-sensitive plays, from a table read from the IR, and drops the EX penalty from those root priors. It solves the scenario. No tuning deck holds an EX-sensitive card, so its row equals the null row game for game. It was in `B_mid`, which is null against `B_core`, and it is off in both presets. Deviation: the rollout policy keeps the EX penalty, so the plan's policy change is done for the root priors only.
- **Evidence:**
  - Scenario test W-09 (T-111; `crates/gcg-search/tests/scenarios/p6e_w09_ex_payment.rs::ex_sensitive_plays_keep_their_ex_payment`, pass): reference `standard` pays with EX in 0/16 seeds (pinned), the null configuration in 0/16, `ex_variants` in 16/16 (bar 14/16).
  - Unit test `mech::p6e::tests::the_ex_sensitivity_table_matches_a_walk_of_the_ir`. The table lists self-sensitive GD04-106, GD04-108 and GD05-118, Command listeners GD04-020, GD04-021 and GD04-085, and exile listener GD05-018.
- **Ablation:**
  - Individual (dev 20005000): 197/400 [0.444, 0.541], -0.015 [-0.080, 0.050], discordant 21/24, 9831.4 vs 9820.6 / 118560.6 vs 117536.8. It is identical to the null row in all 400 games (`docs/port/eval/P6-E/individual-ex_variants.json`).
  - Paired against the null configuration, the no-op baseline of P6-E's paired rows: 188/400 [0.422, 0.519], -0.060 [-0.128, 0.009], 19/31 (`vs-null-ex_variants.json`).
  - Bundle: in `B_mid`.

### W-10 — Equivalent-move merge fingerprint is conservative and depends on instance numbers
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:64-74`, `src/gcg_sim/ai/config.py:72`, `d9d41af`
- **Quotes:** “whether copies merge can depend on their instance numbers, but two different cards are never merged.”; “the merge fingerprint's conservative number checks”
- **What is weak:** `merge_equivalent_moves` compares raw numbers: a once-per-turn record or event value that merely equals a card's uid keeps two interchangeable cards apart, so visits are split unnecessarily; a less conservative port would risk merging moves that are not equivalent.
- **Resolution plan:** in Rust, history events, once-per-turn records and lasting effects carry typed card references (a `CardRef` newtype, distinct from plain integers), so the strengthened fingerprint rewrites only typed references to "self" and never confuses an integer with a uid. The reference port keeps the raw comparison for bit-exact replay.
- **Verification** (unit + property): unit test: two identical copies where an unrelated event value equals one copy's uid are kept apart by the reference (pinned) and merged by the strengthened search; property test over states from 1,000 random games: any two moves the strengthened search merges lead to successor states with equal digests after swapping the two uids.
- **Status:** resolved by measurement (not adopted): `typed_fingerprint` merges moves by typed card references, from a layout table of once keys and event-payload card keys, instead of any value equal to a uid. It is property-tested and null at game level. It was in `B_mid` and is off in both presets. Deviation: the property compares uid-free summaries instead of digests after a uid swap, and it finishes multi-card selections before comparing.
- **Evidence:**
  - Unit test `mech::p6e::tests::typed_fingerprints_ignore_numbers_that_merely_equal_a_uid`: the reference keeps two equivalent attacks apart (pinned) and the typed form merges them.
  - Property `moves_the_typed_merge_groups_lead_to_equivalent_successors`: 1,000 random games on the AI test decks give 4,913 merged pairs, 17 of them merged only by the typed form, all with equal uid-free successor summaries.
- **Ablation:**
  - Individual (dev 20005000): 198/400 [0.446, 0.544], -0.010 [-0.076, 0.056], discordant 22/24, 9835.1 vs 9822.0 / 118598.7 vs 117578.5, with 4 games changed against the null row, +1 -0 (`docs/port/eval/P6-E/individual-typed_fingerprint.json`).
  - Bundle: in `B_mid`.

### W-11 — Pruning and merging run only at the root; no transposition or tree reuse
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/mcts.py:89-110`, `src/gcg_sim/ai/mcts.py:220-235`, `docs/python/AI.md:64-66`
- **Quotes:** “At the root, attacks, blocks, choices and pairings that differ only in which of two”
- **What is weak:** `prune_dominated`, `without_effect` and `merge_equivalent` apply only at the root; below it the tree splits visits between equivalent or dominated moves, and every decision starts a fresh tree.
- **Resolution plan:** strengthened search: (1) in-tree application of the cheap, honest filters for the mover (equivalent-move merge, free-Burst decline, idle 0-AP attacks; not `without_effect`, which is too costly); (2) transposition table within a search keyed by a stable hash of the mover's observation plus the move key; (3) subtree reuse across consecutive decisions of the same player when the observed history since the last decision matches. Hashes come from `derive_seed`-style stable hashing, never process hashing.
- **Verification** (ablation + unit): ablation rows for in-tree filters, transpositions and subtree reuse at equal node budgets; regret per decision with the W-43 tool. Unit test: identical decisions and statistics across runs, thread counts and processes.
- **Status:** resolved by measurement (not adopted): `in_tree_filters`, `transpositions` and `subtree_reuse` are implemented and measured null, and none is in the shipped presets. In the M15 bundle `subtree_reuse` did harm: `B_full` is -0.060 [-0.111, -0.009] against `B_core` on the same deals. Kept visits count toward the budget, so the search ran 24,467 fresh iterations per game against 32,039. `transpositions` was in `B_mid`, which is null against `B_core`.
- **Evidence:**
  - Unit tests `mech::p6d::transpose::tests::keys_ignore_identities_hidden_from_the_searcher` and `two_orders_of_the_same_deployments_share_a_key`.
  - Determinism: `p6d_determinism::every_mechanism_repeats_across_runs_and_thread_counts` and `the_bundle_repeats_in_another_process`.
  - `p6d_reuse::consecutive_own_decisions_start_from_the_kept_subtree`: 90 of 191 searches reused a subtree, 97 of 201 with `transpositions` and 97 of 196 with `root_trees=2`; none did without the flag.
  - `p6d_reuse::the_concrete_search_agent_carries_its_trees` and `mech::p6d::reuse::tests::kept_trees_go_only_to_the_predicted_decision`.
  - All of the above pass.
  - Regret (W-43 tool, 120 positions, oracle `ref:strong` at twice its budget, dev 24600003, `docs/port/eval/P6-D/regret-*.json`):

    | Configuration | Mean regret (SE) | Paired against the null configuration |
    | --- | --- | --- |
    | null configuration | 0.0021 (0.0021) | — |
    | `in_tree_filters` | 0.0042 (0.0025) | +0.0021 [-0.0008, +0.0050] |
    | `transpositions` | 0.0021 (0.0026) | +0.0000 [-0.0029, +0.0029] |

- **Ablation:**
  - Individual rows (dev 24600001, `docs/port/eval/P6-D/`):
    - `in_tree_filters`: 197/400 (0.492, [0.444, 0.541]), -0.015 [-0.082, +0.052], discordant 22/25, 10045.4 vs 9973.7 / 122363.3 vs 120815.7.
    - `transpositions`: 204/400 (0.510, [0.461, 0.559]), +0.020 [-0.051, +0.091], 29/25, 9747.5 vs 9751.2 / 117909.1 vs 118798.4.
    - `transpositions` confirmation (confirm 30460002): 795/1600 (0.497, [0.472, 0.521]), -0.006 [-0.042, +0.029], 104/109.
    - `subtree_reuse`: 201/400 (0.502, [0.454, 0.551]), +0.005 [-0.068, +0.078], 29/28, 9291.8 vs 9830.5 / 112167.9 vs 118419.9.
    - the three together: 205/400 (0.512, [0.464, 0.561]), +0.025 [-0.050, +0.099], 32/27.
  - Bundle rows (dev 21500100, against the null configuration):
    - `B_full`: 193/400 (0.482, [0.434, 0.531]), -0.035 [-0.101, +0.032], 20/27, 24467 vs 31826 / 465773 vs 364277 (`docs/port/eval/M15/rows/dev-bfull-vs-null2x-400.json`).
    - `B_mid`: 212/400 (0.530, [0.481, 0.578]), +0.060 [-0.019, +0.138], 39/27, 31313 vs 31565 / 593929 vs 359693 (`dev-bmid-vs-null2x-400.json`).
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-12 — Root pruning is judged in one fixed determinization by the deterministic policy, with silent caps
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:75-96`, `docs/PROGRESS.md:66-68`, `docs/PROGRESS.md:113-116`, `src/gcg_sim/ai/actions.py:136`, `src/gcg_sim/ai/actions.py:139-150`, `src/gcg_sim/ai/actions.py:214-232`, `src/gcg_sim/ai/noop.py:1-18`, `src/gcg_sim/ai/noop.py:37`, `src/gcg_sim/ai/noop.py:42-52`, `src/gcg_sim/ai/noop.py:72`, `src/gcg_sim/ai/noop.py:77-96`, `src/gcg_sim/ai/noop.py:84`, `src/gcg_sim/ai/noop.py:101-103`, `src/gcg_sim/ai/noop.py:115`, `src/gcg_sim/ai/noop.py:150-151`, `src/gcg_sim/ai/mcts.py:96-107`, `src/gcg_sim/engine/interp.py:1141-1149`, `15ae2eb`, `32e9f84`, `ef05730`, `13fdeff`, `d9d41af`
- **Quotes:** “in one determinization, the move and "end the main phase / pass" are each followed by the deterministic policy to the next turn”; “_TRIAL_WORLD_SEED = 0”; “MAX_DECISIONS = 400”; “correctness bugs in root pruning (no-effect check plays only the policy's target; 0-AP attack prune accepts triggers of other cards and unaffordable tricks; Rest on already-rested targets 27 per 100 games)”; “(c.uid, c.zone, c.rested, c.damage, c.pair, c.known)”
- **What is weak:** `_drop_idle_attacks` and `_resting_changes_more_than_the_unit` trial-apply moves in one seed-0 world (`_TRIAL_WORLD_SEED`). `noop.without_effect` judges every spending move in one determinization too, but that world is seeded from the search RNG (`rng.next_u64()`, one world per decision), not from seed 0. It plays to the next turn, branches only the first SELECT, and compares a fixed signature (zones, rest, damage, pair, known bits, lasting effects, deck order). Its resolution, continuation and replay stop silently at 400 policy moves, and cut lines are compared as if complete. A move whose value appears later, or only in other worlds, can be pruned, and the dominated-move list is hand-coded. A look or reveal effect is never judged without effect, because `_h_look_top` sets `known` bits, which the signature compares. So the pruning risk lies with effects whose result depends on hidden cards but leaves no trace in the signature. Earlier bugs (policy-target-only no-op check, 0-AP prune accepting other cards' triggers and unaffordable tricks, cost-only activations) are fixed behind settings, and `baseline-standard` keeps the buggy behaviour on purpose. The policy-line baseline of the branch check is W-141.
- **Resolution plan:** strengthened search: prune only when the move is a no-op in all of the first K search worlds (K = 4, reusing the common-random-number worlds, so no extra determinization); branch every SELECT in the resolution up to a node cap counted in the budget; a cap hit or a disagreement between worlds keeps the move (fail-safe) with a prior penalty instead of removing it. The reference keeps its single worlds (seed 0 for the attack and Rest-target trials, the search-RNG world for `without_effect`), the first-SELECT branching and the 400 caps.
- **Verification** (scenario + unit + ablation): scenario "hidden-dependent effect". The searcher's spending move matters only in worlds where a hidden card is present, and in the other worlds it leaves no zone, known-bit or lasting trace by the next turn. Example: a "during this turn" protection or stat change on the attacker, which is the searcher's only Unit. It matters only when the Shield the attack destroys holds a damage 【Burst】 such as GD01-111 ("Choose 1 enemy Unit. Deal 2 damage to it."). The opponent's decklist is built so that about half of the worlds hold such a Shield. Before the scenario is used, the builder runs the reference no-effect check over 100 world seeds and confirms that the prune decision differs between worlds. Over 16 agent seeds, the test records how often the reference prunes the move (expected: in about half the seeds, wherever its one world lacks the Burst). The strengthened search must keep it in at least 14/16. Unit test: a forced cap hit keeps the move (strengthened) and is compared as complete (reference, pinned). Ablation row alone and leave-one-out.
- **Status:** resolved by measurement (not adopted): two mechanisms, both scenario-tested and null at game level. Both were in `B_mid` and are off in both presets. The reference keeps its single world, first-SELECT branching and 400 caps.
  - `prune_worlds{K}` judges pruning in each of the first K search worlds, with a fail-safe prior penalty, and treats cap hits as unsure.
  - `branch_all_selects` branches over every SELECT, up to `BRANCH_NODE_CAP = 24`.
- **Evidence:**
  - Scenario test W-12 (T-112; `crates/gcg-search/tests/scenarios/p6e_w12_hidden_burst.rs::the_no_effect_check_keeps_a_move_that_matters_in_some_worlds`, pass). The reference no-effect check prunes Witches from Earth in 52/100 world seeds (pinned). Over agent seeds 1-16, the move is kept by reference `standard` in 8/16 (pinned), by the null configuration in 8/16 and by `prune_worlds=4` in 14/16 (bar 14/16).
  - Unit test `mech::p6e::tests::a_forced_cap_hit_keeps_the_move` passes.
  - Cost: the root judgements take under 1% of a search's applies (`prune_worlds=4`: 909 applies over 42 vector decisions). The core does not count root applies in applies per game (P6-E interface note 2).
- **Ablation:**
  - `prune_worlds=4` (dev 20005000): 196/400 [0.441, 0.539], -0.020 [-0.084, 0.044], discordant 20/24, 9865.6 vs 9834.0 / 118722.0 vs 117395.7, 81 games changed against the null row, +1 -2 (`docs/port/eval/P6-E/individual-prune_worlds.json`).
  - `branch_all_selects`: 197/400 [0.444, 0.541], -0.015 [-0.080, 0.050], 21/24, 9834.7 vs 9827.0 / 118574.8 vs 117556.7, 9 games changed, +0 -0 (`individual-branch_all_selects.json`).
  - Bundle: both in `B_mid`, together with W-141's `real_idle_line`.

### W-13 — The "while rested" Rest-target exception was added after the measurements and never fired at game level
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/python/AI.md:82-86`, `docs/python/AI.md:343-347`, `src/gcg_sim/ai/actions.py:212-232`, `d9d41af`
- **Quotes:** “the fix (5a6f226) adds a check that never fired in 32 games on the test and example decks, which hold no such card”; “Found by the final review (Freedom Gundam GD03-070 made the presets miss a lethal); no card in the test, example or tuning decks has such an effect, and over 32 games on them the new check never fired”
- **What is weak:** the no-op Rest-target prune ignored "while this Unit is rested" effects, so Freedom Gundam GD03-070 made the presets miss a lethal. The fix compares the derived view before and after resting each candidate in one determinization; it never fired in 32 games and the strength numbers predate it.
- **Resolution plan:** the Rust reference reproduces the fixed check bit-exactly. The strengthened search judges the rest-target prune over K worlds (W-12). gcg-eval gets an "effect coverage" deck set containing GD03-070 and every other card whose text reads its own rested state, and the instrumented trace (W-21) counts how often each prune exception fires.
- **Verification** (scenario + e2e): scenario reproducing the GD03-070 missed lethal: reference and strengthened presets both find it 16/16, and a negative control with the exception disabled misses it (shows the scenario discriminates). e2e: 100 gcg-eval games on the coverage decks report a non-zero firing count and a 400-game no-harm guard.
- **Status:** resolved: the reference reproduces the fixed check bit-exactly. `rest_prune_worlds` judges the "while rested" exception in each of the K worlds. It is scenario-tested and measured: identical to the null row on the panel. It was in `B_mid` and is not adopted. The e2e part of the verification is done (`docs/port/eval/weakness-status/GAP-A2.md`):
  - the effect-coverage deck set `docs/port/eval/GAP-A2/decks/`: `coverage-white-blue.txt` holds GD03-070 and every packaged White/Blue card that reads its own rested state, and `coverage-red-green.txt` the Red/Green ones whose rested-state text is a trigger (a census of the 1,152 card numbers found 9 such cards);
  - a per-exception counter in the decision trace: every searched decision of a `--decision-trace` file gets `"rest_prune"` (`pruned`, `rest_trigger`, `while_rested` or `null`), replayed with `gcg_ai::rest_prune` from the canonical moves the root prune receives (`crates/gcg-eval/src/trace.rs`);
  - 100 `gcg-eval` games on the coverage decks with a non-zero count.
- **Evidence:**
  - Scenario test W-13 (T-113; `crates/gcg-search/tests/scenarios/p6e_w13_freedom.rs::both_searches_rest_the_rested_unit_and_keep_the_lethal`, pass): reference `standard` 16/16 (pinned), null configuration 16/16, `rest_prune_worlds` 16/16.
  - Negative control `mech::p6e::tests::without_the_exception_the_freedom_lethal_is_missed`: 0/16.
  - 400-game no-harm guard: the `rest_prune_worlds` row is identical to the null row in all 400 games.
  - e2e test `crates/gcg-eval/tests/gapa2_w13.rs::w13_the_coverage_decks_fire_the_while_rested_exception` (pass): 100 `gcg-eval head-to-head` games on the White/Blue coverage mirror count `while_rested` 110, `pruned` 331 and `null` 9,375. The head-to-head report is byte-identical with and without `--decision-trace`.
  - `gapa2_w13.rs::w13_the_counted_branch_is_the_prune_s_own` (pass): on 40 policy games, `prune_dominated` removes moves exactly when the counter says `pruned`.
  - The recorded run of the shipped `fast` (M15 preset) against `ref:standard` on the White/Blue mirror (dev 22721300, 100 games): the exception fired at 123 of 8,979 searched decisions, in 65 of the 100 games (`docs/port/eval/GAP-A2/w13-coverage-trace-summary.json`, `w13-coverage-fast-vs-ref-standard-100.json`).
- **Ablation:**
  - Individual (dev 20005000): 197/400 [0.444, 0.541], -0.015 [-0.080, 0.050], discordant 21/24, 9831.4 vs 9820.6 / 118560.6 vs 117536.8, 0 games changed (`docs/port/eval/P6-E/individual-rest_prune_worlds.json`).
  - Bundle: in `B_mid`.

### W-14 — Close target choices stay noisy: the <Repair> ping puzzle is solved about a third of the time
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:122-129`, `docs/python/AI.md:222-224`, `docs/python/AI.md:383-389`, `docs/python/AI.md:486-496`, `docs/PROGRESS.md:63`, `tests/ai/test_puzzles_shipped.py:1-3`, `tests/ai/test_puzzles_targets.py:89-116`, `src/gcg_sim/ai/config.py:74`, `bench/__pycache__/positions.cpython-315.pyc`, `d9d41af`, `13fdeff`, `23ddd49:docs/AI.md`
- **Quotes:** “Choosing the less useful of two targets is still possible when their values stay within noise after the extension.”; “only about a third of the time by the shipped `standard` (7 of 24 and 62 of 168 agent seeds in two reviews)”; “screen at 3.0 over 72 games: regret -0.00090 +/- 0.00023 per target decision, 32 better / 6 worse, +9.5% iterations per game”; “target choices have no "do nothing" option to compare against.”
- **What is weak:** `target_extension` (up to 3 budgets while within 2 paired SE) cut regret by 0.00090 ± 0.00023 per target decision at +9.5% iterations, but close targets can still be mis-picked. The <Repair> ping puzzle (ping the Unit that keeps the damage) is solved by the shipped `standard` 7/24 and 62/168, against about 20/24 with `value_scale` 2: the calibrated leaf brings the two targets closer. It is excluded from `tests/ai/test_puzzles_shipped.py`. Not adopted: policy tie-break (worse), fixed target multiplier (−0.00036 ± 0.00019), separate root prior weight (z −1.4 and −1.0). Before #5, target choices had no "do nothing" baseline.
- **Resolution plan:** strengthened search: (1) target critic: for each candidate target, the effect is resolved to its end in each of the first K iteration worlds and scored by the evaluation; the paired score difference seeds the root prior and breaks ties only when the search's own difference is within noise; (2) variance reduction at target decisions: several rollouts per world (counted in nodes) instead of more worlds; (3) a "no target" baseline where the effect allows choosing none.
- **Verification** (scenario + ablation): the Rust port of the <Repair> ping puzzle (`tests/ai/test_puzzles_targets.py:89-116`) with 24 seeds: reference shipped `standard` at most 12/24 (recorded, about 7/24 expected), strengthened `standard` at least 20/24; the puzzle joins the shipped-preset puzzle suite for the strengthened presets. Ablation rows for the critic, multi-rollout and both, with nodes per game.
- **Status:** resolved by measurement (not adopted): plan items (1) and (3) are P6-E's. `target_rollouts` is the target critic. Its final form, v2, speaks only for clock-only differences, needs `CRITIC_Z = 2` over K worlds and scores a DONE option as a target. It solves the puzzle 24/24 and measured null at game level. It was in `B_mid` and is not adopted. The first form, v1, measured negative and was replaced. Plan item (2) is now built and measured (`docs/port/eval/weakness-status/GAP-A1.md`): `target_multi_rollouts{R}` plays R rollouts per iteration at effect-target root decisions, through the new core hook `Mechanisms::rollouts`, and divides the decision's budget by R. It misses the puzzle bar alone, lowers the critic's count, and is null at game level, so it is off in both presets. The puzzle joins the shipped-preset puzzle suite (`crates/gcg-search/tests/ai_puzzles.rs`) only if `target_rollouts` is adopted, which no row supports.
- **Evidence:**
  - Scenario test W-14 (T-114; `crates/gcg-search/tests/scenarios/p6e_w14_target_critic.rs::the_target_critic_solves_the_repair_ping`, pass) ran 24 seeds: reference `standard` 7/24 (pinned; Python recorded 7), null configuration 10/24, `target_rollouts` 24/24 (bar 20/24).
  - Scenario test `crates/gcg-search/tests/scenarios/gapa1_w14_multi_rollout.rs::multi_rollouts_on_the_repair_ping_are_recorded` (T-114, pass), 24 seeds at the reference `standard` budget. `target_multi_rollouts` alone solves 2/24 at R = 2 and 0/24 at R = 4 and 8. With `target_rollouts` it solves 21/24 at each R.
  - Unit tests in `crates/gcg-search/src/mech/gapa1/tests.rs` pass: `one_rollout_per_iteration_is_the_null_configuration`, `multi_rollouts_change_only_target_decisions_and_play_r_rollouts_per_iteration` and `the_budget_is_divided_by_the_rollouts_rounding_up`.
  - In the frozen WK scenarios of W-17, the shipped `standard` (M15 presets) solves the <Repair> ping (`wk7_damage_the_unit_that_cannot_repair`) 16/16, the shipped `fast` 3/16 and the reference 5/16 (`docs/port/eval/GAP-A1/weakness/scenarios-report.json`).
- **Ablation:**
  - Multi-rollout rows (individual, dev 22710300, `docs/port/eval/GAP-A1/rows/`): null configuration 210/400 (0.525, [0.476, 0.574]), +0.050 [-0.020, +0.119], discordant 31/21. `target_multi_rollouts=4` 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.040, +0.109], 33/26, 8855 vs 9917 / 116654 vs 119391. `=2` 205/400 (0.512), +0.025 [-0.044, +0.094], 28/23. Both (`=4` with `target_rollouts`) 206/400 (0.515), +0.030 [-0.045, +0.105], 33/27, 8841 vs 9934 / 116221 vs 119466. Against the null row on the same deals each is -0.0075 to -0.0125 per game.
  - `target_multi_rollouts=4` confirmation (confirm 30271300, run once): 820/1600 (0.512, [0.488, 0.537]), +0.025 [-0.010, +0.060], 115/95, 8848 vs 9829 / 115874 vs 117853 (`confirm-individual-target_multi_rollouts-4-1600.json`).
  - v2 individual (dev 20005000): 199/400 [0.449, 0.546], -0.005 [-0.072, 0.062], discordant 23/24, 9831.6 vs 9832.3 / 118602.9 vs 117773.6, 46 games changed against the null row, +2 -0 (`docs/port/eval/P6-E/individual-target_rollouts.json`).
  - v2 paired against the null configuration: 188/400 [0.422, 0.519], -0.060 [-0.126, 0.007], 18/30. That is the no-op row's result: 30 games changed against it, +1 -1 (`vs-null-target_rollouts.json`).
  - v1 individual: 195/400 [0.439, 0.536], -0.025 [-0.091, 0.042], 21/26, 276 games changed, +6 -8.
  - v1 paired: 180/400 [0.402, 0.499], -0.100 [-0.169, -0.029], 17/37, +3 -11 against the no-op row (`*-v1.json`).
  - Bundle: v2 in `B_mid`.

### W-15 — Undocumented rollout cap of 250 decisions: capped rollouts are evaluated mid-turn
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `src/gcg_sim/ai/config.py:33`, `src/gcg_sim/ai/mcts.py:211-214`, `src/gcg_sim/ai/playout.py:12-26`, `src/gcg_sim/ai/baseline.py:31`
- **Quotes:** “max_rollout_decisions: int = 250”
- **What is weak:** `play_until_turn` stops silently at the cap and the static evaluation then scores the state wherever it is (possibly mid-battle or mid-resolution); the cap is not in `docs/python/AI.md` and its frequency was never measured.
- **Resolution plan:** gcg-eval instrumentation counts capped rollouts per decision for both searches (trace only). The strengthened search continues a capped rollout with the deterministic policy to the next main-phase decision for at most a further fixed number of decisions and, if still unsettled, scores it 0.5 instead of a mid-resolution estimate. The Rust docs for the strengthened presets document the cap.
- **Verification** (unit + e2e): unit test with a tiny cap: the trace records the cap hit and the strengthened leaf is taken at a main-phase decision; e2e: capped-rollout rate per preset on 100 gcg-eval games is reported; if it is non-zero, an ablation row decides the fallback.
- **Status:** resolved: `capped_rollout_continuation` is adopted in both shipped presets as a guard. The 250-decision cap was never hit on the panel: 0 capped rollouts in every row. So it changes no game there and costs nothing while the cap is not hit. The Rust `docs/AI.md` documents the cap and the continuation (`docs/AI.md:172-176`, written in Phase 8, commit 8c89b5e).
- **Evidence:**
  - Unit tests `capped_rollouts_continue_to_a_main_phase_decision` and `an_unsettled_capped_line_scores_one_half` pass.
  - The e2e capped-rollout rate is 0 per decision. That holds for `search:null-standard` and `ref:standard`, about 15,000 searched decisions per agent in each 400-game row. It also holds for `search:standard` at twice its pre-M15 budget (`docs/port/eval/weakness-status/P6-A.md`).
- **Ablation:**
  - Individual (dev 20100100): 208/400 (0.520, [0.471, 0.569]), +0.040 [-0.033, +0.112], discordant 32/24, 9946 vs 9924 / 120263 vs 119879. It is identical to the placebo game for game.
  - High budget (dev 20100200, the family placebo): 193/400 (0.482, [0.434, 0.531]), -0.035 [-0.108, +0.038], 25/32, 32722 vs 32810 / 381007 vs 381122.
  - Leave-one-out (dev 21500200, the family placebo): 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.032, +0.101], 27/20, 32778 vs 32388 / 598554 vs 579017 (`docs/port/eval/M15/rows/dev-loo-capped_rollout_continuation-400.json`).

### W-16 — Budget is fixed by option count; only effect-target decisions may extend
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/config.py:55-57`, `src/gcg_sim/ai/config.py:80-111`, `src/gcg_sim/ai/mcts.py:121-150`, `src/gcg_sim/ai/mcts.py:172-202`
- **Quotes:** “wanted = self.iterations_per_option * n_options”; “return max(self.min_iterations, min(self.max_iterations, wanted))”
- **What is weak:** budget is `clamp(iterations_per_option·n, min, max)` (fast 7n in [16, 64], standard 64n in [160, 640], strong 128n in [320, 1280]); only target decisions extend (up to 3 budgets at z = 2); early stop when the leader cannot be overtaken. Critical decisions (lethal on board, last Blocker) get no more nodes than routine ones; generic near-tie extensions were null at equal iterations.
- **Resolution plan:** strengthened search: critical-position extension: MAIN, ATTACK, BLOCK and ACTION_STEP decisions where the threat solver (W-03/W-17) reports lethal for either side within the horizon get up to 2 extra budgets while the top two moves are within z paired SE; non-critical decisions keep the base budget. Deterministic and counted in nodes.
- **Verification** (scenario + ablation): scenario: the tactical lethal puzzles at a reduced budget (reference `fast`-sized) where the reference fails at least half of 16 seeds, strengthened `fast` solves at least 14. Ablation row with nodes per game, alone and leave-one-out.
- **Status:** resolved by measurement (not adopted): `critical_extension` is implemented and measured null for about 27% more iterations, so it is off in both presets. `lethal_root` solves the scenario's lethal at the `fast`-sized budget. It is null at game level and also off (W-17). Neither is in the shipped presets (`docs/port/eval/weakness-status/P6-B.md`).
- **Evidence:**
  - Scenario test W-16 (T-115; `crates/gcg-search/tests/scenarios/p6b_w16_fast_lethal.rs`, pass) ran 16 agent seeds: reference `fast` 0/16 (pinned; bar ≤ 8), reference `standard` 5/16, `fast` + `critical_extension` 0/16, `fast` + `lethal_root` 16/16 (bar ≥ 14).
  - Unit test `critical_decisions_get_two_extra_budgets_and_root_trees_share_the_solver_cap` passes.
- **Ablation:**
  - `critical_extension` individual (dev 20200100): 210/400 (0.525, [0.476, 0.574]), +0.050 [-0.020, +0.119], discordant 31/21, 12565 vs 9611 / 150118 vs 115744, vs placebo -0.0025 [-0.0202, +0.0152].
  - High budget: 206/400 (0.515, [0.466, 0.564]), +0.030 [-0.042, +0.101], 30/24, 40527 vs 32284 / 463948 vs 375353.
  - Confirmation (confirm 30200200): 795/1600 (0.497, [0.472, 0.521]), -0.006 [-0.044, +0.032], 119/124, 41036 vs 32213 / 475116 vs 375627 (`docs/port/eval/P6-B/rows/confirm-hb-critical_extension-1600.json`).
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-17 — Remaining implausible plays: passivity, questionable attacks, missed lethals; the audit sample is small
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/python/AI.md:391-398`, `docs/python/AI.md:441`, `docs/PROGRESS.md:107-108`, `docs/PROGRESS.md:159-163`, `docs/status.json:42-46`, `scripts/blind_transcripts.py:1-17`, `bench/__pycache__/weakness.cpython-315.pyc`, `bench/__pycache__/positions.cpython-315.pyc`, `bench/__pycache__/probes.cpython-315.pyc`
- **Quotes:** “passivity (5 → 6), questionable attacks (3 → 4) and two missed lethals in the new games show what remains.”; “The 2026-10 audit used five BO1 games per example pairing for each AI version”; “category of the weaknesses listed in docs/AI.md”
- **What is weak:** blinded audit: 19 confirmed implausible plays in 15 new-standard games (22 for the old AI): passivity 6, questionable attacks 4, wasted effects 4, bad targets 2, plus two missed lethals; five BO1 games per pairing cannot show a significant change, and the classified list lived in the gitignored `build/aiq2/audit/classified.md`. The lost weakness benchmark grouped weaknesses as WK1 lethal detection, WK2 lethal avoidance, WK3 attack/block, WK4 trick waste, WK5 resource choices, WK6 mulligan, WK7 target choices, WK8 long precise lines.
- **Resolution plan:** (1) strengthened search: exact this-turn lethal solver at the root (bounded DFS over deploy, pair, removal and attack orders with engine-correct blockers; lethal must hold in every one of the first K worlds), which plays a found lethal or gives it a dominant prior; (2) gcg-eval `weakness` suite: frozen scenarios per WK category (including the lost `hold_up`, `ex_variant`, `rest_the_active_unit`, `damage_the_unit_that_cannot_repair`, `burst_aware_attack_order`, `pair_the_linking_pilot`), scored for reference and strengthened presets; (3) a larger blinded audit (20 BO1 games per example pairing) with transcripts from a Rust port of `scripts/blind_transcripts.py`, raw outputs committed under the eval data directory; (4) recover the missed-lethal positions. Their only record was the gitignored `build/aiq2/audit/classified.md`, which is absent along with the generator `build/aiq2/audit/gen.sh` (W-127), and `docs/python/AI.md:397` gives only the count. gcg-eval regenerates the 15 new-`standard` audit games: reference `standard` at d9d41af, BO1, five games per example pairing, seeds 90031–90033, with decision logs. The command and the seed-to-pairing mapping are reconstructions, so they are recorded with the regenerated files. The exact lethal solver of step (1) then runs at every main-phase and attack decision of these games, from the decider's information (lethal in every one of the first K worlds). Each decision where such a lethal exists and the reference's line does not win that turn becomes a frozen scenario.
- **Verification** (scenario + ablation): one scenario per WK category, each recording the reference's failure rate and asserting the strengthened bar (16 seeds). Every missed-lethal scenario recovered by step (4) must be found 16/16 by strengthened `standard`. The number recovered is reported next to the audit's two, and is not asserted to be two, because the regenerated games cannot be checked against the lost ones. If none is recovered, the entry says so, and the lethal bar rests on the W-16 lethal puzzles. Ablation row for the lethal solver alone and leave-one-out; the audit's confirmed-implausible count per game is reported with a Wilson interval.
- **Status:** resolved: the blinded audit of step (3) ran on the shipped M15b presets (`docs/port/eval/final-2/audit/README.md`). Five auditor agents that did not generate the games classified blinded transcripts: full reads of a stratified sub-sample (6 games per AI version) and every rule-flagged turn of the 60 games per version. Confirmed (`clear`) implausible plays per fully read game: shipped `standard` 1.33 (SE 0.21), reference `standard` 2.50 (SE 0.56); games with at least one confirmed play 6/6 for both [0.610, 1.000]. Games with a confirmed missed lethal: shipped `standard` 0/60 [0.000, 0.060], reference `standard` 2/60 [0.009, 0.114]. Deviation from the plan: the full reads cover 12 of the 120 games, because reading all 120 transcripts (about 10 MB) was out of proportion; the rule-flagged turns cover all 120. The frozen scenarios rescored with the M15b presets: shipped `fast` and `standard` 336/352 (0.955), reference 266/352 (0.756). The earlier steps (`docs/port/eval/weakness-status/GAP-A1.md`):
  - Step (1), `lethal_root`, the exact this-turn lethal solver at the root, is implemented with scenario evidence. It measured null and is not adopted.
  - Step (2), the `gcg-eval weakness scenarios` suite: 22 frozen WK scenarios (WK1-WK8, including the six the lost benchmark named), scored for the reference and the shipped presets.
  - Step (3): the deterministic sample of 20 BO1 games per example pairing for the reference and the shipped `standard` (M15 sample `docs/port/eval/GAP-A1/audit/runs/`; the M15b `standard` games, same masters, `docs/port/eval/final-2/audit/runs/search-standard/`), its rule-based candidate count (`gcg-eval weakness audit`), the blinded transcripts (`gcg-eval transcripts`, W-119) and the auditors' classifications (`docs/port/eval/final-2/audit/auditors/`, unblinded by `tally_audit.py` into `audit-result.json`).
  - Step (4), the regenerated audit games, recovered no real missed lethal.
- **Evidence:**
  - The audit (`docs/port/eval/final-2/audit/`): `prepare_audit.py` (packets), `INSTRUCTIONS.md` (the auditors' brief), `auditors/auditor-1.json` to `auditor-5.json`, `key.json`, `assignment.json`, `audit-report.json` (rule-based candidates on the M15b sample: shipped `standard` 10 flagged turns, 3 confirmed; reference 16, 8 confirmed), `audit-result.json`. Confirmed plays by category, full reads: shipped `standard` passivity 5, bad target 2, other 1; reference passivity 8, bad target 3, other 2, questionable attack 1, wasted effect 1.
  - Frozen scenarios rescored with the M15b presets (`docs/port/eval/final-2/audit/scenarios-report.json`, `gcg-eval weakness scenarios --agents ref:standard,search:fast,search:standard --seeds 16`): reference 266/352 (0.756, [0.708, 0.798]), shipped `fast` 336/352 (0.955, [0.927, 0.972]), shipped `standard` 336/352 (0.955, [0.927, 0.972]).
  - Frozen scenarios (`docs/port/eval/GAP-A1/weakness/scenarios-report.json`, 22 scenarios x 16 seeds, M15 presets): reference `standard` 266/352 (0.756, [0.708, 0.798]), shipped `fast` 307/352 (0.872, [0.833, 0.903]), shipped `standard` 320/352 (0.909, [0.875, 0.935]), shipped `fast` with `tree_lethal_select` 323/352 (0.918, [0.884, 0.942]). The shipped presets solve every WK1 lethal and the WK8 two-turn plan. No configuration solves WK5's EX payment, which needs `ex_variants` (W-09, not adopted). The shipped `standard`'s 0/16 on the WK7 removal-target puzzle is a construction effect: the position gives the searcher no Shields, so every target loses and the root values tie (GAP-A1).
  - `crates/gcg-search/tests/scenarios/gapa1_weakness_scenarios.rs::the_frozen_weakness_scenarios_are_current` (T-116) and `the_audit_solver_proves_the_wk1_lethals_and_no_others` pass, and so do the `crates/gcg-eval/src/weakness_suite.rs` tests `the_committed_scenarios_cover_every_category_and_score_with_controls` and `the_audit_replays_a_recorded_game_and_its_flags_are_deterministic`.
  - The audit sample's rule-based count (`docs/port/eval/GAP-A1/weakness/audit-report.json`, masters 22710601-22710603, 60 games per AI version): reference `standard` 6 missed-lethal candidates (4 real on the true state), 5 wasted effects, 0 self-harm targets, flags in 9/60 games [0.081, 0.261]; shipped `standard` (M15 preset) 6 (5 real), 2, 0, 7/60 [0.058, 0.222]. These are candidates for the auditors, not their verdict.
  - WK1, lethal detection: scenario W-16 (`lethal_root` 16/16 against reference `standard` 5/16 and `fast` 0/16), scenario W-32 (b) (reference `standard` 0/16, the strengthened configuration 16/16) and unit test `the_solver_plays_a_removal_before_the_attack`.
  - WK2, lethal avoidance: scenario W-03.
  - No missed lethal was recovered, so the lethal bar rests on these puzzles, which the frozen scenarios include.
  - Step (4), `mech::p6b::audit::every_real_missed_lethal_of_the_regenerated_audit_games_is_found` (feature `slow`): 15 regenerated BO1 games of reference `standard`, masters 90031-90033 (a reconstruction). 31 decisions had a lethal proved in every world. The reference's line won that turn in all but one, 90032:0 turn 15, which is not lethal on the true state. Recovered real missed lethals: 0, against the audit's two.
- **Ablation:**
  - `lethal_root` individual (dev 20200100): 210/400 (0.525, [0.476, 0.574]), +0.050 [-0.017, +0.117], discordant 29/19, 9162 vs 9660 / 113513 vs 116178, vs placebo -0.0025 [-0.0110, +0.0060], with 81 games changed.
  - High budget: 206/400 (0.515, [0.466, 0.564]), +0.030 [-0.040, +0.100], 29/23, 29816 vs 32180 / 357828 vs 373833.
  - Confirmation (confirm 30200200): 791/1600 (0.494, [0.470, 0.519]), -0.011 [-0.049, +0.026], 114/123, 30110 vs 32244 / 363616 vs 375795 (`docs/port/eval/P6-B/rows/confirm-hb-lethal_root-1600.json`). It saves 7% of the iterations.
  - The three solver flags together (`p6b_solver`): individual 214/400 (0.535, [0.486, 0.583]), +0.070 [-0.000, +0.139]; high budget 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.037, +0.107].
  - Leave-one-out: not applicable (not in the shipped bundle).
  - The suite, the sample and the count only play the existing agents and read states, so they have no row (A13).

### W-18 — `fast` rates are not `standard` rates; fidelity rests on one weakened deck
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/python/AI.md:400-440`, `.claude/skills/gcg-benchmark/SKILL.md:39-41`, `.claude/skills/gcg-benchmark/SKILL.md:88-97`, `tests/ai/test_fast_preset.py:1-42`, `docs/status.json:12`, `docs/status.json:47-53`, `d9d41af`
- **Quotes:** “`fast` rates are not `standard` rates”; “fast preset tests: its budget is at most 1/4.5 of standard's at every decision width and it differs from standard only in budget and noise”
- **What is weak:** on red-green-zeon vs blue-white-federation the fast-minus-standard paired difference is +0.068 [+0.011, +0.124]; fast vs standard 156/400, vs greedy 251/400; clamp(6n, 16, 64) was rejected; screening fidelity rests on one deliberately weakened deck (141/200 vs 139/200).
- **Resolution plan:** strengthened `fast` starts from reference `standard` budgets (about 10x the old `fast`), which the Rust engine makes affordable. gcg-eval `fidelity`: for each example pairing plus at least three weakened variants across colours, run `fast` and `standard` on the same deals and report paired differences and the rank correlation of variant win rates; the screening funnel keeps "confirm survivors with `standard`".
- **Verification** (e2e): fidelity report on held-out seeds: wherever strengthened `standard` resolves a matchup, strengthened `fast` favours the same deck, and every paired difference interval includes 0 or is below 0.05 in magnitude; Rust slow-suite port of `tests/ai/test_fast_preset.py` (budget ratio and settings parity).
- **Status:** open: the verification is not met on held-out seeds with the shipped M15b presets. The run (eval master 90300007, run once, `docs/port/eval/final-2/fidelity-fast-standard.json`, checked by `docs/port/eval/GAP-A2/w18_fidelity_check.py`) agrees on the ranking (Spearman 0.9, Kendall tau-b 0.8, pairwise agreement 0.9, the same best variant `red-green-zeon`), and wherever `standard` resolves a matchup `fast` favours the same deck. One of the five paired differences fails the interval part: `blue-white-federation-weakened`, `fast` 59/200 against `standard` 74/200, -0.075 [-0.146, -0.003], so the interval excludes 0 and is not within 0.05. The other four include 0. Closing it needs a `fast` that tracks `standard` on that variant (for example a larger `fast` budget), which changes the evaluated preset and so needs its own tuning rows and a new evaluation. The `fast` mapping (it starts from the reference `standard`), the `gcg-eval fidelity` tool and the slow-suite port are done. The same protocol met the verification on the pre-M15 presets (P6-D) and on the M15 presets (`docs/port/eval/weakness-status/GAP-A2.md`), both on confirm-role seeds (T-131).
- **Evidence:**
  - `crates/gcg-search/tests/scenarios/p6d_fast_preset.rs` passes: `fast_searches_at_most_half_of_the_standard_budget_at_every_width` (n = 1..40), `fast_differs_from_standard_only_in_budget_and_noise` and `weakened_variants_used_for_fidelity_are_legal`.
  - Fidelity on the M15 presets (confirm 30721801, run once, 200 games per variant and agent, `docs/port/eval/GAP-A2/fidelity-fast-standard-m15.json`, checked by `docs/port/eval/GAP-A2/w18_fidelity_check.py`):

    | Variant | `fast` wins | `standard` wins | Paired difference |
    | --- | --- | --- | --- |
    | `red-green-zeon` | 92/200 (0.460) | 98/200 (0.490) | -0.030 [-0.100, +0.040] |
    | `red-green-zeon-weakened` | 48/200 | 38/200 | +0.050 [-0.007, +0.107] |
    | `purple-white-tekkadan` | 80/200 | 81/200 | -0.005 [-0.080, +0.070] |
    | `purple-white-tekkadan-weakened` | 34/200 | 46/200 | -0.060 [-0.123, +0.003] |
    | `blue-white-federation-weakened` | 72/200 | 71/200 | +0.005 [-0.059, +0.069] |

    Spearman 0.9, Kendall tau-b 0.8, pairwise agreement 0.9, same best variant. Wherever `standard` resolves a matchup (4 of 5 variants), `fast` favours the same deck, and every paired difference interval includes 0. The verification is met for these presets.
  - Fidelity on the pre-M15 presets (confirm 30460001, 200 games per variant and agent, `docs/port/eval/P6-D/fidelity-fast-standard.json`):

    | Variant | `fast` wins | `standard` wins | Paired difference |
    | --- | --- | --- | --- |
    | `red-green-zeon` | 112/200 (0.560) | 108/200 (0.540) | +0.020 [-0.051, +0.091] |
    | `red-green-zeon-weakened` | 48/200 | 45/200 | +0.015 [-0.048, +0.078] |
    | `purple-white-tekkadan` | 95/200 | 92/200 | +0.015 [-0.062, +0.092] |
    | `purple-white-tekkadan-weakened` | 48/200 | 48/200 | +0.000 [-0.068, +0.068] |
    | `blue-white-federation-weakened` | 73/200 | 76/200 | -0.015 [-0.084, +0.055] |

    Spearman 0.9747, Kendall tau-b 0.9487, pairwise agreement 1.0, same best variant.
- **Ablation:**
  - E2 (`search:fast` against `ref:standard`), first final evaluation (M15 presets, not accepted): 6171/11200 (0.5510, [0.5418, 0.5602]), held-out 0.5544, no pair with an upper bound below 0.5 (`docs/port/eval/final-1/README.md`). Final evaluation 2 pending (`docs/port/eval/final-2/`).
  - `fast` budget curve against `ref:standard` (tuning 11500200): 0.5x 0.487, 0.75x 0.517, 1x 0.525, 2x 0.560, and 1.5x 228/400 (0.570, [0.521, 0.618]), +0.140 [+0.062, +0.215], discordant 47/19, 14822 vs 10088 / 281059 vs 121316 (`docs/port/eval/M15/curves/tuning-fast-bundle-vs-ref-standard.json`).
  - Sanity row of the shipped `fast` (dev 21500410, 420 games): 223/420 (0.531, [0.483, 0.578]), +0.062 [-0.012, +0.134], 38/25, 14656 vs 9909 / 270697 vs 117941 (`docs/port/eval/M15/rows/dev-sanity-fast-vs-ref-standard-420.json`).

### W-19 — Strength acceptance bars are thin floors
- **Category:** ai-search · **Resolution type:** eval tooling (acceptance gates)
- **Sources:** `docs/python/AI.md:351-361`, `docs/SPEC.md:58`, `docs/status.json:12`, `docs/status.json:47-53`, `tests/ai/test_puzzles.py:22`, `tests/ai/test_puzzles_shipped.py:25`, `tests/ai/test_puzzles_targets.py:89-92`, `tests/slow/test_ai_strength.py:48`, `tests/slow/test_ai_strength.py:56`
- **Quotes:** “these results are floors, not the measure of improvement.”; “SEEDS = (1, 2)”; “292/400 vs greedy (Wilson 95% [0.684, 0.771])”
- **What is weak:** criterion 7a needs only at least 90% against random and a Wilson lower bound above 0.5 against greedy (measured 400/400 and 292/400 = 0.730); greedy shares the role weights; tactical puzzles use two agent seeds and the target puzzle requires 16/24.
- **Resolution plan:** the Rust reference keeps criterion 7a unchanged (equivalence). The strengthened presets get their own gates in gcg-eval: strengthened `standard` vs reference `strong` and strengthened `fast` vs reference `standard`, each Wilson lower bound above 0.5 on a held-out seed run once; every tactical puzzle with 8 agent seeds and a per-puzzle bar recorded with the scenario.
- **Verification** (e2e): Rust slow tests for both sets of gates; a gate below its bar fails the suite.
- **Status:** resolved: the strengthened presets have their own gates, and this entry's bars hold on held-out seeds run once (final evaluation 2, `docs/port/eval/final-2/README.md`): `standard` against reference `strong` 0.572 [0.563, 0.581], `fast` against reference `standard` 0.558 [0.549, 0.568], each lower bound above 0.5, held-out pairs alone included (0.548 and 0.540). `gcg-eval gates` computes and checks E1-E4 (A26, stricter than this entry's plan), and its test passes; on final evaluation 2 it passes E2 and E3 (all 14 panel losses to random documented with verified replays) and fails E1's held-out pooled check (0.548 against 0.55) and E4's test-deck check (304/400, lower bound 0.716 against 0.764), which are the brief's acceptance results, reported in `docs/port/eval/notes.md`. The first gate run (M15 presets, `docs/port/eval/final-1/`) was not accepted. The reference keeps criterion 7a (`crates/gcg-eval/tests/strength_reference.rs`, T-070). The per-puzzle bars are recorded with each scenario test.
- **Evidence:**
  - `crates/gcg-eval/tests/phase6.rs::w19_gates_compute_e1_to_e4_and_fail_below_a_bar` (pass):
    - reports above every bar pass;
    - E1 with 205/400 on the held-out pairs fails the held-out bound while the pooled-all bound passes ("gates not passed: E1", exit 1);
    - one loss to random fails E3 until it is listed with an existing replay;
    - a report with the wrong opponent and a `dev` role fails its validity checks.

    The tool is `crates/gcg-eval/src/gates.rs`.
  - The first final evaluation (`docs/port/eval/final-1/README.md`): E1 pooled lower bound 0.5504 (brief bar 0.55: pass) but held-out 0.5413 (plan A26: fail); E2 pass; E3 400/400 on the test decks, and 27 panel losses to random not documented with replays (fail); E4 test decks 307/400, lower bound 0.7237 (bar 0.764: fail), panel lower bound 0.7649 (pass by 0.0009), held-out 0.7597 (fail). `gcg-eval gates --check` reports overall fail.
- **Ablation:** no AI effect (row I). The tool reads reports.

### W-20 — Strength evidence rests on few decks, and the tuning decks overlap the example decks
- **Category:** ai-search · **Resolution type:** eval tooling (deck panel)
- **Sources:** `docs/python/AI.md:228-231`, `docs/python/AI.md:329-342`, `docs/python/AI.md:351-353`, `src/gcg_sim/ai/tuning.py:64-93`, `tests/ai/decks.py:14`, `tests/ai/decks.py:38`, `tests/ai/decks.py:71`, `examples/decks/red-green-zeon.txt`, `examples/decks/purple-white-tekkadan.txt`, `examples/decks/blue-white-federation.txt`, `tests/bench/__pycache__/test_preset_evidence.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “132 of 240 = 0.550 [0.487, 0.612]”; “The tuning decks are different from the AI test decks.”; “DECKS: tuple[DeckList, DeckList] = (FEDERATION_DECK, SEED_DECK)”
- **What is weak:** the headline 925/1600 = 0.578 is on two test decks, and the example-deck check is not significant (45, 46, 41 of 80 per pairing). Overlaps were verified this session by multiset intersection of main-deck copies. Tuning decks against example decks: tuning Zeon shares 27/50 with red-green-zeon, tuning Tekkadan 29/50 with purple-white-tekkadan, and tuning Wing 2/50 with blue-white-federation. Test decks against example decks: test deck 0 (Blue/White Federation, `FEDERATION_DECK`) shares 27/50 with blue-white-federation and 3/50 with purple-white-tekkadan. Test deck 1 (Red/White SEED, `SEED_DECK`) shares 3/50 with blue-white-federation and 2/50 with red-green-zeon. So the example-deck spot check (132/240) is independent of neither the tuning decks nor the test decks. The artifacts sweep attributed the tuning finding to a "PD20" note in the cited bytecode; that string is not in the file, so the claim rests on the recomputation.
- **Resolution plan:** gcg-eval deck panel: a fixed, versioned set of legal decks (curated lists plus a seeded generator over colour pairs), split into tuning and held-out halves. No held-out deck may share more than 10/50 main-deck copies with any tuning deck or any AI test deck. All strengthened-vs-reference evidence is reported per panel deck and pooled with a stratified interval. The example decks are reported separately and labelled by their overlap with the tuning and test decks, never as held-out evidence.
- **Verification** (unit + e2e): a unit test computes the overlaps and reproduces, on the current files, 27/50, 29/50 and 2/50 (tuning against example decks) and 27/50, 3/50, 3/50 and 2/50 (test against example decks). It enforces the panel bound against both deck sets. e2e: panel head-to-head report.
- **Status:** resolved: the data part comes from P6-decks: five new legal decks of distinct archetypes in `examples/decks/eval/`, `docs/port/eval/panel.json` and `examples/decks/eval/PAIRS.md`. The held-out decks are `blue-red-tempo` and `green-white-link`, which form 13 held-out pairs; the other 15 are tuning pairs. The tooling part comes from P6-I: `gcg-eval decks --overlap [--check]`, `by_deck`, and a pair-stratified interval in every head-to-head and row report. The eval-role panel report is part of the final evaluation (`docs/port/eval/final-1/`, not accepted, and `docs/port/eval/final-2/`, running).
- **Evidence:**
  - `crates/gcg-eval/tests/phase6.rs::w20_overlaps_reproduce_the_recorded_values` gives 27/50, 29/50, 2/50, 27/50, 3/50, 3/50 and 2/50.
  - `w20_panel_head_to_head_and_the_overlap_bound`.
  - `uv run python port/oracle/eval_decks.py --check`, which is current and runs in `scripts/verify_all_rust.sh --full`.
  - `gcg-eval decks --overlap --check` passes (P6-N gate).
  - The decks pass `gcg-sim validate` in Python and Rust (T-128).
  - `--role dev` refuses a held-out pair.
- **Ablation:** no AI effect (row I and P6-decks): deck files, `panel.json` and a script change which decks are played, not how any agent decides. Tier-1 corpus 3804/3804.

### W-21 — The decision log keeps the top 5 alternatives and does not record search effort
- **Category:** ai-search · **Resolution type:** eval tooling (instrumentation)
- **Sources:** `src/gcg_sim/ai/base.py:15`, `src/gcg_sim/ai/base.py:84`, `bench/legacy_ai/__pycache__/instrumented.cpython-315.pyc`
- **Quotes:** “LOG_ALTERNATIVES = 5”; “The decision log keeps only the top alternatives”
- **What is weak:** the log cannot show how many iterations a decision used, extension rounds, early stops or capped rollouts, so equal-cost comparisons needed a separate instrumented agent.
- **Resolution plan:** keep the Python decision-log format byte-identical (equivalence). gcg-eval writes a separate per-decision trace (all root moves with visits and means, nodes used, early stop, extension rounds, capped rollouts, prune exceptions fired) to its own file, never to `games.ndjson` or replays.
- **Verification** (unit + e2e): unit test: trace node counts sum to the nodes the search used and list every root move; e2e: `games.ndjson` and replays are byte-identical with tracing on and off.
- **Status:** resolved: the decision log stays Python's. `--decision-trace FILE` on `gcg-eval head-to-head`, `reference-benchmark` and `preset-benchmark` writes one NDJSON line per decision. Each line holds every root move, unranked and unrounded, with iterations, budget, cap, `extended`, `early_stop`, applies by purpose, worlds and capped rollouts. Prune exceptions are not traced, because the agents do not expose them.
- **Evidence:** `crates/gcg-eval/tests/phase6.rs::w21_decision_traces_change_no_report_and_count_every_iteration`. The report is byte-identical with and without the trace. Root visits sum to the iterations. `preset-benchmark` writes byte-identical `games.ndjson`, `results.json`, `summary.md` and replays.
- **Ablation:** no AI effect (row I). `traced_games_equal_plain_games_for_every_agent_kind` shows identical outcomes for random, greedy, the reference and the strengthened search.

## AI policy

### W-22 — Greedy baseline: one determinization, baseline knobs, fixed choices, silent cap
- **Category:** ai-policy · **Resolution type:** port-preserve + eval tooling
- **Sources:** `docs/python/AI.md:8`, `docs/python/AI.md:79-81`, `docs/python/AI.md:149-150`, `docs/ASSUMPTIONS.md:88`, `docs/ASSUMPTIONS.md:94`, `src/gcg_sim/ai/policy.py:50-60`, `src/gcg_sim/ai/agents.py:18-19`, `src/gcg_sim/ai/agents.py:35-66`
- **Quotes:** “greedy keeps the older, looser test”; “greedy always uses them”; “GREEDY_MAX_DECISIONS = 200”
- **What is weak:** one-ply look-ahead on one determinization with `BASELINE_KNOBS` and `DEFAULT_WEIGHTS` (now the role weights, so it plays differently than before); the turn completion stops at 200 decisions and capped lines are scored mid-turn; it always goes first and uses 48 mulligan samples. Results against it are floors.
- **Resolution plan:** greedy stays a frozen reference baseline (criterion 7a depends on it) and is ported bit-exact; improvement claims are made against reference presets (W-19), not greedy. gcg-eval counts greedy cap hits in its trace.
- **Verification** (e2e + unit): equivalence: Rust greedy vs Python greedy on fixed seeds give identical games and decision logs; unit test that a forced cap hit is counted.
- **Status:** resolved: greedy stays the frozen, bit-exact port. The decision trace counts its cap hits (`trace::greedy_cap_hits`, which replays greedy's own computation from its per-decision seed).
- **Evidence:** `crates/gcg-eval/tests/phase6.rs::w22_greedy_cap_hits_are_counted` (a forced cap of 0 cuts moves at three MAIN decisions; the real cap of 200 cuts none). The `baselines` corpus set is 8/8 (T-065), within tier 1 at 3804/3804.
- **Ablation:** no AI effect (row I). Greedy is unchanged, and the count is a replay outside the agent.

### W-23 — Hand-written playout policy: Commands under-cast, abilities over-activated, unfitted constants
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:145-163`, `docs/python/AI.md:453-461`, `src/gcg_sim/ai/policy.py:1-8`, `src/gcg_sim/ai/policy.py:42-47`, `src/gcg_sim/ai/policy.py:175-183`, `src/gcg_sim/ai/policy.py:187-250`, `src/gcg_sim/ai/policy.py:288-298`, `src/gcg_sim/ai/config.py:67`, `src/gcg_sim/ai/config.py:75-76`, `d9d41af`, `81a45d9`, `23ddd49:docs/AI.md`
- **Quotes:** “Rollouts and priors come from a hand-written policy.”; “s = 2.5 + 0.3 * V.cdef(st, o.a).cost - cost”; “effect-aware Command scoring and priors distilled from search statistics made decisions worse in the investigation and were not built.”; “Scores every legal option of any decision from the current board without look-ahead”
- **What is weak:** measured: rollouts cast Commands in 33% of turns where search casts them in 45%, and activate abilities 55% of the time against search's 22% (the pre-#5 docs said the opposite, "plays Commands somewhat eagerly"). Scores are hand-set: `EX_PENALTY = 1.5`, `LETHAL = 60`, `CLEAR_THE_WAY = 8`, `MUST_BLOCK = 100`, `KEEPABLE_TOP = 2`, `IDLE_ATTACK = -1`, PLAY_COMMAND `2.5 + 0.3·cost` (+8 for removal when blocked from lethal), ACTIVATE flat 1.5 (Support 0.5). Effect-aware Command scoring and priors distilled from search statistics made decisions worse and were not built; the Burst, 0-AP face-attack and rule-polarity fixes shipped.
- **Resolution plan:** strengthened presets use a frequency-calibrated policy: a handful of scalar offsets (PLAY_COMMAND base, ACTIVATE base, Support, end-phase pass preference, EX penalty) fitted by deterministic coordinate search so that rollout action frequencies match the strengthened search's own choice frequencies on its self-play games (moment matching on 4–6 scalars, not a distilled prior over all features, which was measured worse). The fitted table is a constant of each strengthened preset; the reference keeps the Python constants.
- **Verification** (ablation + e2e): gcg-eval reports the rollout-vs-search frequency gap for Commands and activations on the deck panel (target: within 5 points each); ablation rows: calibrated policy alone and leave-one-out.
- **Status:** resolved by measurement (not adopted): `calibrated_policy` adds six offsets to the reference scores. They come from a conditional-logit IRLS fit in f64 on 7,102 MAIN and ACTION_STEP decisions of 120 self-play games (tuning 10600001); the weights file is `docs/port/eval/P6-F/policy-fit.json`. The policy measured negative at 400 games and is off in both presets.
- **Evidence:**
  - The frequency gap (target ≤ 5 points) out of sample, on 1,655 decisions of 30 dev games (master 20600001):

    | Move kind | Search | Reference policy | Calibrated | Gap |
    | --- | --- | --- | --- | --- |
    | Commands | 13.2% | 11.2% | 12.8% | 0.4 points |
    | Activations | 2.3% | 19.1% | 6.2% | 3.9 points |

  - These tests pass: `fitted_constants_are_the_recorded_fit`, `fit_selfplay::fitted_policy_reproduces` (feature `slow`, 599 s, the fit reproduces byte for byte), `fit::tests::irls_recovers_a_known_model_deterministically` and `cholesky_solves_a_small_system`.
- **Ablation:**
  - Individual (dev 20600100): 197/400 (0.492, [0.444, 0.541]), -0.015 [-0.086, +0.056], discordant 25/28, 9805 vs 9868 / 118356 vs 118618, vs placebo -0.0050 [-0.0471, +0.0371] (`docs/port/eval/P6-F/rows/dev-calibrated_policy-400.json`).
  - All ten P6-F flags: 184/400 (0.460, [0.412, 0.509]), -0.080 [-0.154, -0.005], 22/38.
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-24 — Modal choices are scored flat
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:455-456`, `src/gcg_sim/ai/policy.py:349-351`, `d9d41af`
- **Quotes:** “modal choices are scored flat”
- **What is weak:** when the decision is a mode choice every option scores 0, so rollouts pick modes uniformly (up to temperature) and priors for mode choices are uniform.
- **Resolution plan:** strengthened policy scores each mode by a one-ply look-ahead: resolve the mode on a clone of the rollout world (nested choices by the deterministic policy) and score the evaluation difference; results cached per (program, mode, state version); cost counted in nodes.
- **Verification** (scenario + unit + ablation): unit test: on a modal Command with one useless mode, reference rollout mode frequencies are about 50/50 (recorded), strengthened picks the useful mode at least 90% at rollout temperature. Scenario where the search's value of holding vs casting a modal Command depends on rollouts choosing the right mode later; 16 seeds, strengthened at least 13, reference recorded. Ablation row.
- **Status:** resolved by measurement (not adopted): `modal_lookahead` resolves each mode on a clone and scores it. The package kept it as non-negative, but it was null in `B_mid` against `B_core`, so it is off in both presets.
- **Evidence:**
  - Unit test `modal_lookahead_picks_the_useful_mode` uses Wings of Light (GD05-102) with 20,000 draws at temperature 0.5. The reference splits 9,977 / 10,023 (pinned); `modal_lookahead` picks the useful mode 20,000 / 0 (bar ≥ 90%).
  - Scenario test W-24 (T-117; `crates/gcg-search/tests/scenarios/p6f_w24_modal.rs`, synthetic cards in `docs/port/eval/P6-F/synthetic_modal_fragment.json`, pass) runs at the reference `fast` budget. The reference deploys the modal Unit 0/16 (pinned) and `modal_lookahead` 16/16 (bar ≥ 13).
- **Ablation:**
  - Individual (dev 20600100): 202/400 (0.505, [0.456, 0.554]), +0.010 [-0.063, +0.083], discordant 29/27, 9817 vs 9750 / 118289 vs 117910, vs placebo +0.0075 [-0.0034, +0.0184].
  - The five non-negative P6-F flags together (`p6f_kept`): 204/400 (0.510, [0.461, 0.559]), +0.020 [-0.049, +0.088], 27/23, 9937 vs 9965 / 245751 vs 120744.
  - Bundle: in `B_mid`.

### W-25 — The opponent is modelled with the searcher's own policy
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:456-457`, `docs/python/AI.md:512-513`, `src/gcg_sim/ai/mcts.py:205-217`
- **Quotes:** “the opponent is modelled with the same policy”; “A multiple-observer search, or opponent models other than the shared policy, if a larger budget makes opponent nodes worth splitting.”
- **What is weak:** rollouts for both seats call the same policy with the same knobs, so the opponent is exactly as passive or eager as the searcher's own heuristic; other opponent models are deferred.
- **Resolution plan:** opponent modelling in the strengthened search: (1) separate opponent rollout knobs (temperature and the calibrated offsets of W-23) chosen by ablation, including a lower-temperature "competent opponent"; (2) optional per-deck opponent profile: the W-23 calibration run on reference self-play of the benchmark deck, stored per deck digest and used only when the opponent's decklist matches; (3) belief-weighted worlds (W-07) use the same opponent model. All deterministic.
- **Verification** (scenario + ablation): scenario "over-extension": a line that loses to a punishing reply the shared policy plays only rarely; 16 seeds, reference over-extends in at least 8 (recorded), strengthened at most 3. Ablation rows per opponent setting and leave-one-out.
- **Status:** resolved: `tree_lethal_select` is adopted in both shipped presets (M15b, `docs/port/eval/M15b/README.md`; `crates/gcg-search/src/presets.rs` `SHIPPED_MECHANISMS`). At an in-tree MAIN decision of the turn player, either side, the node selects the first move of a lethal line that P6-B's solver proves on the iteration's world. So below the root the opponent is no longer modelled only by the shared policy's priors where a lethal reply exists. With `tactical_solver` (adopted at M15) the rollouts' opponent plays proved lethals too. The over-extension scenario (T-118) now reproduces the weakness, and both shipped presets meet its bar. The other opponent settings (`docs/port/eval/weakness-status/GAP-A1.md`):
  - Plan item (1), `opponent_knobs`, is implemented, unit-tested and measured negative, so it is not adopted. It gives the opponent's rollouts the W-23 offsets at a quarter of the rollout temperature.
  - Plan item (2), the per-deck opponent profile, is not built, by GAP-A1's recorded decision: it would specialise `opponent_knobs`, which measured negative and over-extends 16/16, while the competent opponent the scenario needs comes from the solver, which needs no fit.
  - Plan item (3), belief weighting by this model, is not built, by the same record: `belief_worlds` (W-07) weighs worlds with the reference policy, no fitted opponent model is adopted, and the solver plays only proved lethal lines, which the weighted decision never involves.
- **Evidence:**
  - Scenario test W-25 (T-118; `crates/gcg-search/tests/scenarios/gapa1_w25_over_extension.rs::the_reference_over_extends_and_a_competent_opponent_model_does_not`, pass), P6-B's last-Blocker position, 16 agent seeds, the defender `ref:standard`. Over-extensions at HEAD `a926ac7` (run 2026-11-04, `cargo test --release -p gcg-search --test scenarios gapa1_w25 -- --nocapture`): reference `standard` 16 (pinned; bar at least 8), null configuration 16, `opponent_knobs` 16, `tactical_solver` 0, `tactical_solver` + `adaptive_horizon` 15, shipped `fast` 0, shipped `standard` 0 (bar at most 3), `tree_lethal_select` alone 0. Before M15b the shipped `fast` over-extended 16/16 (GAP-A1).
  - Unit test `mech::gapa1::tests::tree_lethal_select_is_deterministic_and_spends_counted_solver_applies` passes: on the 126 searched D2 vector decisions the flag is deterministic, and every changed search ran the solver.
  - `presets.rs` test `shipped_presets_enable_the_m15b_bundle` and `crates/gcg-search/tests/null_config.rs` pass (the null configuration still replays the reference; `docs/port/progress.md`, M15b gate).
  - Unit test `opponent_knobs_model_the_opponent_by_its_own_policy` passes. Over 20,000 draws the shared policy picks the best target 11,279 times and the opponent model 18,866; the searcher's own moves are unchanged.
  - The predictive temperature fit (3.21) is recorded in `docs/port/eval/P6-F/policy-fit.json` and not used.
- **Ablation:**
  - `tree_lethal_select` individual (dev 22710300): 217/400 (0.542, [0.493, 0.591]), +0.085 [+0.016, +0.153], discordant 34/17, 10002 vs 9930 / 126284 vs 119213. Confirmation (confirm 30271800, run once): 843/1600 (0.527, [0.502, 0.551]), +0.054 [+0.018, +0.089], 128/85, 10222 vs 9887 / 128675 vs 118656 (`docs/port/eval/GAP-A1/rows/confirm-individual-tree_lethal_select-1600.json`).
  - Leave-one-out from the M15 shipped `fast` (dev 22710800): +0.010 [-0.063, +0.083]; confirmation (confirm 30271810): 817/1600 (0.511, [0.486, 0.535]), +0.021 [-0.014, +0.056], 112/95, 14565 vs 14125 / 271686 vs 261804.
  - Leave-one-out from `B_core` at twice the pre-M15 `standard` budget (dev 22710810): +0.040 [-0.029, +0.108]; confirmation (confirm 30271820): 806/1600 (0.504, [0.479, 0.528]), +0.007 [-0.029, +0.044], 114/108, 33544 vs 32824 / 612374 vs 603871.
  - The M15b bundle (both M15b flags) against the M15 `standard` at the shipped budget (confirm 30500001): 831/1620 (0.513, [0.489, 0.537]), +0.026 [-0.010, +0.062], discordant 120/99, 127323 vs 122195 / 2285589 vs 2194745, no pair with an upper bound below 0.5 (`docs/port/eval/M15b/rows/confirm-std-b-vs-std-1620.json`). Against greedy on the same deals (dev 20500001): M15b `standard` 338/420, M15 `standard` 328/420. M15b adopted both flags because every component is non-negative (PLAN §12.4). A leave-one-out row of `tree_lethal_select` from the M15b bundle itself is not recorded.
  - `opponent_knobs` individual (dev 20600100): 196/400 (0.490, [0.441, 0.539]), -0.020 [-0.088, +0.049], discordant 23/27, 9801 vs 9816 / 117884 vs 118816, vs placebo -0.0075 [-0.0516, +0.0366] (`docs/port/eval/P6-F/rows/dev-opponent_knobs-400.json`). Leave-one-out: not applicable (not in the shipped bundle).

### W-26 — Kill-aware and damage-payoff targeting were measured and removed
- **Category:** ai-policy · **Resolution type:** AI (strengthened search, re-measure)
- **Sources:** `docs/python/AI.md:457-461`, `docs/ASSUMPTIONS.md:92`, `docs/PROGRESS.md:143-145`, `d9d41af`
- **Quotes:** “made no difference to the search's choices (+0.00025 ± 0.00032 against a reseed placebo), so they were removed”; “kill-aware and damaged-payoff targeting were measured and dropped (squashed from ws/ai-policy)”
- **What is weak:** the policy's own Damage picks improved (regret −0.020 per Damage decision) but the search's choices did not change, so the settings were removed; rollouts keep picking Damage targets without regard to kills or follow-up payoffs.
- **Resolution plan:** re-implement both as strengthened-policy options and re-measure under the strengthened search, where the target critic (W-14), longer horizon (W-01) and calibrated policy (W-23) change what rollouts feed back; adopt only under the adoption rule, otherwise record the null row and leave them out again.
- **Verification** (ablation): regret per Damage decision with the W-43 tool (policy-level and search-level) and an ablation row alone and leave-one-out.
- **Status:** resolved: kill-aware targeting is re-implemented as the policy option `kill_aware_targeting`, re-measured under the strengthened search, and adopted in both shipped presets (M15b, `docs/port/eval/M15b/README.md`; `crates/gcg-search/src/presets.rs` `SHIPPED_MECHANISMS`). The re-measurement inside the M15 bundle was confirmed above zero (`docs/port/eval/weakness-status/GAP-A1.md`). The policy-level regret per Damage decision is measured with `gcg-eval weakness damage-regret`: the flag lowers it. Damage-payoff targeting is not rebuilt, by GAP-A1's recorded decision: its Python rule was lost in the squash, and on the panel only `red-purple-control` holds a card that reads a damaged enemy Unit (3 copies of GD01-111), so a row could not tell it from its placebo at 400 games.
- **Evidence:**
  - Policy-level regret (`docs/port/eval/GAP-A1/weakness/damage-regret-report.json`): every Damage decision of 120 `ref:standard` self-play games on the tuning pairs (dev 22710200), oracle values over 8 shared worlds, 484 decisions. Regret per decision at temperature 0: reference policy 0.0754 (se 0.0059), `kill_aware_targeting` 0.0537 (se 0.0051), difference -0.0217 [-0.0324, -0.0110]. Under the rollout's sampling the difference is -0.0210 [-0.0307, -0.0113]. This reproduces the Python investigation's -0.020.
  - Unit test `weakness_suite::tests::a_damage_decision_is_judged_by_both_policies` passes.
  - Search-level regret per target decision (W-43 tool, 157 target positions, 8 shared worlds, oracle `ref:strong`, dev 20600300, `docs/port/eval/P6-F/rows/regret-*.json`): `search:null-standard` -0.0048 (se 0.0044), with `kill_aware_targeting` -0.0016 (se 0.0049). The difference is within noise.
  - `presets.rs` test `shipped_presets_enable_the_m15b_bundle` and `crates/gcg-search/tests/null_config.rs` pass (`docs/port/progress.md`, M15b gate).
- **Ablation:**
  - Individual (dev 20600100): 216/400 (0.540, [0.491, 0.588]), +0.080 [+0.009, +0.150], discordant 35/19, 9865 vs 9883 / 118485 vs 120365, vs placebo +0.0425 [-0.0003, +0.0853].
  - Individual confirmation (confirm 30600100, run once): 817/1600 (0.511, [0.486, 0.535]), +0.021 [-0.015, +0.057], 116/99, 9998 vs 9886 / 119288 vs 119093 (`docs/port/eval/P6-F/rows/confirm-kill_aware_targeting-1600.json`).
  - Leave-one-out from `B_core` at twice the pre-M15 `standard` budget (dev 22710100): 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.040, +0.109], discordant 33/26. Confirmation (confirm 30271100, run once): 830/1600 (0.519, [0.494, 0.543]), +0.037 [+0.001, +0.074], 129/99, 32860 vs 32490 / 601667 vs 606589 (`docs/port/eval/GAP-A1/rows/confirm-loo-kill_aware_targeting-bcore2x-1600.json`).
  - The M15b bundle against the M15 `standard`: as W-25 (confirm 30500001, 831/1620, +0.026 [-0.010, +0.062]). A leave-one-out row of `kill_aware_targeting` from the M15b bundle itself is not recorded.

### W-27 — YES_NO answers say no about 12% of the time; the policy always goes first and keeps
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:161-163`, `src/gcg_sim/ai/policy.py:305-318`, `src/gcg_sim/ai/agents.py:55-56`
- **Quotes:** “Yes (score 1 against 0, so a rollout at temperature 0.5 says no about 12% of the time).”; “return [1.0 if o.kind is A.YES else 0.0 for o in dec.options]”
- **What is weak:** Bursts were fixed (`policy_burst_yes_score` 4.0, 0.03% declines; W-163 covers the assumption behind that fix) but generic YES_NO was not, and "always yes" may be wrong for optional costs. CHOOSE_FIRST always picks itself and REDRAW always keeps; greedy always goes first.
- **Resolution plan:** strengthened policy scores YES_NO by the one-ply look-ahead of W-24: a free beneficial "you may" gets a Burst-like high score, an optional cost is accepted only when the look-ahead says it pays. Pre-game choices of search agents use the setup procedures (W-44, W-45); policy and greedy stay as in the reference.
- **Verification** (unit + ablation): unit tests on rollout distributions: free beneficial YES_NO accepted at least 99.9%; a harmful optional cost (e.g. a "you may discard 1" whose payoff is idle) declined at least 90% (reference 88% yes, pinned). Ablation row.
- **Status:** resolved by measurement (not adopted): `yes_no_lookahead` answers a "you may" by one-ply look-ahead, or with the Burst score when the body is harmless. It measured negative and is off in both presets. The pre-game half of the title (always going first and keeping) is handled under W-44 and W-45.
- **Evidence:** unit test `yes_no_lookahead_declines_an_idle_cost_and_accepts_a_free_benefit` passes, with 20,000 draws for each case:
  - the idle optional cost of GD02-094: the reference says YES 17,655/20,000 = 88.3% (pinned); `yes_no_lookahead` says YES 42/20,000;
  - the free benefit of EB01-030: `yes_no_lookahead` says YES 19,994/20,000 = 99.97%.
- **Ablation:**
  - Individual (dev 20600100): 193/400 (0.482, [0.434, 0.531]), -0.035 [-0.105, +0.036], discordant 23/30, 9843 vs 9788 / 120967 vs 117852, vs placebo -0.0150 [-0.0427, +0.0127] (`docs/port/eval/P6-F/rows/dev-yes_no_lookahead-400.json`).
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-28 — End-phase action-step plays are under-explored by rollouts
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:157`, `docs/python/AI.md:375-376`
- **Quotes:** “Tricks during a battle are tried; passing is preferred in the end phase.”
- **What is weak:** rollouts prefer passing in the end-phase action step, so lines that cast removal or rest effects there (the puzzle where the agent casts removal in the opponent's end step so the Blocker stays rested) are found only by tree search, not by rollouts.
- **Resolution plan:** strengthened policy scores end-phase action-step plays with the one-ply look-ahead (W-24) instead of a flat pass preference, limited to plays whose effect outlasts the turn (rest, destroy, damage, lasting effects) so rollouts do not waste tricks.
- **Verification** (scenario + ablation): the "hold removal" puzzle at a reduced budget (reference `fast`-sized): reference solves at most 8/16 (recorded), strengthened `fast` at least 14/16. Ablation row.
- **Status:** resolved by measurement (not adopted): `end_phase_actions` scores an end-phase action-step play against passing. Both lines are played through the decider's next turn. It solves the scenario, measured negative and is off in both presets.
- **Evidence:**
  - Unit test `end_phase_lookahead_plays_the_rest_in_the_opponents_end_phase` passes.
  - Scenario test W-28 (T-119; `crates/gcg-search/tests/scenarios/p6f_w28_end_phase.rs`, synthetic card in `docs/port/eval/P6-F/synthetic_end_phase_fragment.json`, pass): the reference wins 8/16 (pinned; bar ≤ 8) and `end_phase_actions` 16/16 (bar ≥ 14).
  - The packaged "hold removal" puzzle does not reproduce the weakness: the reference solves it 16/16 at the `fast` budget.
- **Ablation:**
  - Individual (dev 20600100): 193/400 (0.482, [0.434, 0.531]), -0.035 [-0.109, +0.040], discordant 26/33, 9681 vs 9924 / 129760 vs 120378, vs placebo -0.0150 [-0.0523, +0.0223].
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-29 — No policy scorer for ORDER_TRIGGER decisions
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/policy.py:104-108`, `src/gcg_sim/ai/policy.py:388-400`, `src/gcg_sim/engine/types.py:107`
- **Quotes:** “if scorer is None:”; “ORDER_TRIGGER = "order_trigger"”
- **What is weak:** trigger ordering has no scorer: greedy and deterministic rollouts take the first option, softmax rollouts are uniform, and search priors for these decisions are uniform.
- **Resolution plan:** strengthened policy: ORDER_TRIGGER scored by look-ahead (resolve each order with the deterministic policy and evaluate) when there are at most 6 orders, otherwise a heuristic (own beneficial triggers first, draw before discard, damage before Repair on the same Unit).
- **Verification** (scenario + unit): scenario with two simultaneous triggers whose order matters (draw-then-discard vs discard-then-draw): reference picks option order (pinned), strengthened picks the better order in at least 15/16 seeds; unit test for the heuristic ordering.
- **Status:** resolved by measurement (not adopted): `order_trigger_scorer` resolves each order on a clone when there are at most 6, and uses a heuristic otherwise. It solves the scenario, measured negative and is off in both presets.
- **Evidence:**
  - Unit test `order_heuristic_draws_before_discarding_and_damages_before_repair` passes.
  - Scenario test W-29 (T-120; `crates/gcg-search/tests/scenarios/p6f_w29_order.rs`, synthetic cards in `docs/port/eval/P6-F/synthetic_order_fragment.json`, pass): the reference deploys the trigger Unit 7/16 (pinned) and `order_trigger_scorer` 16/16 (bar ≥ 15).
- **Ablation:**
  - Individual (dev 20600100): 197/400 (0.492, [0.444, 0.541]), -0.015 [-0.087, +0.057], discordant 26/29, 9739 vs 9742 / 120940 vs 117611, vs placebo -0.0050 [-0.0148, +0.0048], with 33 games changed.
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-30 — Target polarity classification is coarse
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/targeting.py:41-42`, `src/gcg_sim/ai/targeting.py:51`, `src/gcg_sim/ai/targeting.py:58-61`, `src/gcg_sim/ai/targeting.py:126-137`, `src/gcg_sim/ai/targeting.py:148-160`, `tests/ai/test_policy_knobs.py:96-105`
- **Quotes:** “otherwise every "can't" rule”
- **What is weak:** without the rule table (greedy and the baseline) every `cant_` rule counts as harmful and every other rule as helpful, and the test lists sites the prefix rule signs wrongly; even with the table, CUSTOM rules, unsigned PLAY_COST/LEVEL_DELTA and StatMods with zero or non-integer totals are UNKNOWN, and only the first harmful, helpful or Apply step on the chosen variable is read.
- **Resolution plan:** strengthened policy: an exhaustive polarity table in Rust (a `match` over every rule kind, so a new kind fails to compile until signed), CUSTOM rules signed per custom id, PLAY_COST/LEVEL_DELTA signed by the amount's sign, non-integer StatMods resolved by look-ahead; all steps acting on the chosen variable are combined.
- **Verification** (unit + ablation): unit test: no rule kind or custom id in the registry classifies as UNKNOWN; the mis-signed sites listed in `tests/ai/test_policy_knobs.py:96-105` classify correctly. Ablation row.
- **Status:** resolved by measurement (not adopted): `exhaustive_polarity` signs every `RuleKind` with an exhaustive `match`. It signs amount-signed kinds by their amount, and CUSTOM rules through `CUSTOM_RULES` (the packaged registry has none). A choice still unknown is scored by look-ahead. The package kept it, but it was null in `B_mid`, so it is off in both presets.
- **Evidence:** unit tests `every_granted_rule_and_custom_id_is_signed` and `exhaustive_polarity_signs_the_listed_sites_and_computed_effects` pass.
- **Ablation:**
  - Individual (dev 20600100): 201/400 (0.502, [0.454, 0.551]), +0.005 [-0.070, +0.080], discordant 30/29, 9763 vs 9676 / 121405 vs 116881, vs placebo +0.0050 [-0.0200, +0.0300].
  - Bundle: in `B_mid`.

### W-31 — Heuristic card and fight values are rough
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/values.py:1`, `src/gcg_sim/ai/values.py:13-47`, `src/gcg_sim/ai/values.py:65-71`, `src/gcg_sim/ai/values.py:83-87`
- **Quotes:** “Rough worth of a Unit or Base on the field, in "stat points".”
- **What is weak:** a flat `KEYWORD_BONUS` of 0.3 per keyword whatever the keyword; `card_worth` uses only type, cost and stats and ignores text and board; `fight()` handles only AP against remaining HP and First Strike (no damage prevention or reduction, redirect, Breach, triggers or tricks); life counts Base HP / 3. Shared by the policy and the setup choices.
- **Resolution plan:** strengthened policy: fight outcomes come from the engine's own battle damage resolution on a clone (exact prevention, reduction, First Strike and Breach), and keyword worth comes from the per-keyword evaluation features fitted in W-37 instead of a flat bonus.
- **Verification** (unit + ablation): unit test: strengthened fight outcomes equal engine battle results on a corpus of attacker/defender pairs including damage-reduction and First Strike cards (reference `fight()` disagreements recorded); ablation row.
- **Status:** resolved by measurement (not adopted): the fight half is done. `exact_fight` resolves the attack and block scorers' fights as engine battles on clones. It measured null (+0.005) at twice the applies per game, was left out of `B_full` for its cost and is not adopted. The keyword half is closed by the decision recorded in `docs/port/eval/weakness-status/GAP-A2.md`: the flat `KEYWORD_BONUS` of 0.3 stays. The only fitted per-keyword worths are the weights of W-37's `keyword_features` models, and they are associations of the fit, not keyword values. Their only game-level use measured negative (W-37).
- **Evidence:**
  - Unit test `exact_fights_equal_the_engine_battles` passes. On 98 attacker/defender pairs, including First Strike, damage reduction, damage immunity and 【Attack】 triggers, the exact fight equals the engine battle on every pair. The reference `fight()` disagrees on 2 (pinned).
  - `docs/port/eval/GAP-A2/w31_keyword_worth.py` converts the W-37 weights (`docs/port/eval/P6-G/fit-round1.json`, true-state model) into the policy's units. Repair (-0.40 and -0.36) and Support (-0.27 and -0.41) are worth less than nothing to their owner in both seats. Breach, Blocker and High-Maneuver change sign between the seats. Suppression is worth 2.1 to 3.4 units, almost a whole 3 AP / 3 HP Unit (3.75). The determinised model gives the same signs.
- **Ablation:**
  - Individual (dev 20600100): 201/400 (0.502, [0.454, 0.551]), +0.005 [-0.063, +0.073], discordant 25/24, 9726 vs 9770 / 243522 vs 118208, vs placebo +0.0050 [-0.0209, +0.0309] (`docs/port/eval/P6-F/rows/dev-exact_fight-400.json`).
  - The keyword half's rows are W-37's `keyword_features` rows: individual (dev 20700100) 194/400 (0.485), -0.030 [-0.100, +0.040]; high budget (dev 20700200) 184/400 (0.460), -0.080 [-0.159, +0.000] (`docs/port/eval/P6-G/rows/`).
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-32 — AI Blocker counts ignore CANT_BLOCK and NO_BLOCKER_VS
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/values.py:74-80`, `src/gcg_sim/ai/evaluation.py:173-176`, `src/gcg_sim/ai/evaluation.py:249-253`, `src/gcg_sim/engine/battle.py:46-75`, `docs/port/inventory/08-ai-policy-eval.md:1319-1322`
- **Quotes:** “and not V.rules_of(dv, u, d.RuleKind.CANT_BLOCK)”; “Active Units of ``player`` with <Blocker> (candidates to intercept an attack).”; “`_race` ignores rested Blockers.”
- **What is weak:** `ready_blockers` (used by the policy's board and block-risk scoring) and the evaluation's per-side Blocker count take every active <Blocker>. The engine's `eligible_blockers` also excludes CANT_BLOCK and the attacker's NO_BLOCKER_VS rules. `_race` checks CANT_BLOCK but not NO_BLOCKER_VS, and it counts every defender Blocker, rested or not, while `_side.blockers` and `ready_blockers` count only active ones (verified in the code this session). The port inventory flags this asymmetry as undocumented. It may be deliberate, since the race assumes the boards stay as they are and the defender's Units stand up.
- **Resolution plan:** strengthened policy and evaluation use one `effective_blockers(attacker)` helper that calls the engine's eligibility rules. The race (W-38) asks the same helper which Blockers are active on each simulated turn. The reference keeps its counts, including `_race`'s rested Blockers.
- **Verification** (unit + scenario): unit test: a Unit under CANT_BLOCK, an attacker with NO_BLOCKER_VS, and (for the race) a rested Blocker are excluded by the strengthened helper and counted by the reference (pinned). Scenario: lethal is on because the only enemy Blocker cannot block. The reference search sees the engine's real block options in its tree and may already find the lethal, so the test records the reference `standard` search's solve count over 16 seeds, not only what the reference policy does. The scenario counts as a reproducer only if the reference solves at most 8/16, and the strengthened `standard` must solve 16/16. If the reference already solves it, W-32 rests on the unit test and the W-38 ablation row, and the scenario stays as a no-regression check.
- **Status:** resolved: `board::effective_blockers` reads Blockers through the engine's `procedures::eligible_blockers`, so CANT_BLOCK, NO_BLOCKER_VS, <High-Maneuver>, CANT_BE_BLOCKED and rested Units count as the engine counts them. The adopted `tactical_solver` plays the engine's own block options, so the shipped presets' solver is CANT_BLOCK-aware. The leaf-feature flag `effective_blockers` measured null and is not adopted. The reference keeps `values.ready_blockers` and `_race` (pinned). No packaged card grants NO_BLOCKER_VS.
- **Evidence:**
  - Unit test `engine_blockers_exclude_cant_block_where_the_reference_counts_it` (Girty Lue, GD05-127; the reference counts are pinned).
  - Scenario test W-32 (T-121; `crates/gcg-search/tests/scenarios/p6b_w32_cant_block.rs`, pass) ran 16 agent seeds at the reference `standard` budget:
    - (a) reference `standard` 16/16, reference `fast` 16/16, `lethal_root` + `effective_blockers` 16/16. This is the no-regression check.
    - (b) reference `standard` 0/16 (pinned; bar ≤ 8), reference `fast` 0/16, the strengthened configuration 16/16 (bar 16/16).
- **Ablation:**
  - `effective_blockers` individual (dev 20200100): 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.032, +0.101], discordant 27/20, 9795 vs 9700 / 117279 vs 115982, vs placebo -0.0100 [-0.0425, +0.0225].
  - High budget: 202/400 (0.505, [0.456, 0.554]), +0.010 [-0.068, +0.087], 33/31, 32129 vs 32499 / 372538 vs 379164 (`docs/port/eval/P6-B/rows/hb-effective_blockers-400.json`).
  - The rows measure a feature swap under `ROLE_WEIGHTS`. The solver's use of the helper is covered by `tactical_solver`'s leave-one-out row (W-03).

### W-33 — AI module-level caches are unbounded and not invalidated on script override
- **Category:** ai-policy · **Resolution type:** Rust fix (no behaviour change)
- **Sources:** `src/gcg_sim/ai/targeting.py:54-55`, `src/gcg_sim/ai/targeting.py:93-112`, `src/gcg_sim/ai/actions.py:167-199`, `src/gcg_sim/effects/registry.py:209-215`, `src/gcg_sim/testkit.py:212`
- **Quotes:** “_harm_cache: dict[int, bool] = {}”
- **What is weak:** `_harm_cache` is keyed by `def_id`; `override_script` (tests) changes a card's program but only the listen cache is cleared, so a stale removal classification is possible in tests; the `(program_id, pc)` caches are safe because programs are append-only but are process-global mutable state.
- **Resolution plan:** in Rust these caches belong to the registry (or a per-search context) and carry the registry generation; an override bumps the generation. No process-global mutable caches.
- **Verification** (unit): override a card's Command program in a test registry and assert the harm classification follows the new program; both searches' outputs unchanged on the equivalence seeds.
- **Status:** resolved by construction (Phase 3): the port has no process-global AI table. `command_harms_enemy_units`, `choice_polarity`, `choice_rule_lasts_this_battle` and `chosen_only_to_rest` recompute from the `&Registry` they are given. Every registry, including each `with_fragment` override, has a fresh `generation` (`docs/port/eval/weakness-status/P6-O.md`).
- **Evidence:** `crates/gcg-search/tests/ai_caches.rs` passes:
  - `per_instruction_classifications_follow_each_registrys_program`: two ST01-014 overrides share program id 626, and the classification follows each one, in the orders A-B-A and B-A-B;
  - `every_override_is_a_new_registry_generation`.

  Both searches' outputs: the `search` and `baselines` corpus sets at 100%, and the null-configuration tests.
- **Ablation:** no AI effect. It is built in by construction, with no off switch. Evidence per A13: the `search` and `baselines` sets at 100% against Python's traces, and the null-configuration tests.

## AI evaluation

### W-34 — Evaluation fitted on pre-change search games; refit and per-preset fits deferred
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:165-183`, `docs/python/AI.md:218-219`, `docs/python/AI.md:226-256`, `docs/python/AI.md:475-478`, `docs/python/AI.md:507-508`, `docs/ASSUMPTIONS.md:92`, `docs/PROGRESS.md:143-145`, `src/gcg_sim/ai/evaluation.py:1-13`, `src/gcg_sim/ai/evaluation.py:104-129`, `d9d41af`
- **Quotes:** “refitting to the new search's own games is a follow-up.”; “test whether `strong` and `fast` benefit from their own fits.”; “a refit of the 14 base weights on the same data was worse out of fold (+0.0025) and did not ship.”; “ROLE_WEIGHTS: the roles feature set fitted on 540 standard-preset self-play games on the tuning decks (7,941 positions; CV log-loss 0.0090 below TUNED_WEIGHTS”
- **What is weak:** `ROLE_WEIGHTS` (the default for every preset) come from self-play of the pre-change `standard` search with `TUNED_WEIGHTS` (seed 35001, 540 games, 7,941 positions; L2 λ = 1, Newton's method) but serve the new presets; all presets share them. A standard-budget refit of the 14 base weights was worse out of fold and did not ship (`TUNED_WEIGHTS` stays for `baseline-standard`).
- **Resolution plan:** Rust tuning tool in gcg-eval (port of `tuning.py`, same fit, made deterministic and worker-count independent) refits the role features on strengthened `standard` self-play at the strengthened leaf point, and separately for strengthened `fast`; iterate fit → self-play → refit until held-out log-loss stops improving (at most 3 rounds). Weights become constants of the strengthened presets; the reference keeps `ROLE_WEIGHTS` and `TUNED_WEIGHTS`.
- **Verification** (ablation + e2e): log-loss table (out of fold by game, test decks, held-out panel decks of W-20) must improve on `ROLE_WEIGHTS`; ablation rows per preset (refit vs `ROLE_WEIGHTS`) alone and leave-one-out.
- **Status:** resolved by measurement (not adopted): `gcg-eval tune-leaf` (`crates/gcg-eval/src/tune_ext.rs`) records Rust self-play of the strengthened search at the strengthened leaf point. It fits by a deterministic Newton method, with CV by game, lambda by CV and a held-out pair. `refit_weights` measured negative at game level and is off in both presets. One fit serves both presets: a `fast`-only fit gains 0.0014-0.0035 nats, which is recorded as the decision. The rounds stopped after round 1, because the held-out log-loss stopped improving. The reference keeps `ROLE_WEIGHTS` and `TUNED_WEIGHTS`.
- **Evidence:**
  - Log-loss (`docs/port/eval/P6-G/fit-round1.json`), out of fold on determinised / true-state samples: `ROLE_WEIGHTS` 0.5578 / 0.5500, `refit_weights` 0.5338 / 0.5282 (lambda 1).
  - On the held-out pair: 0.5857 to 0.5768.
  - On the unseen `fast` self-play positions (`fit-standard-selfplay-test-fast.json`): 0.5663 to 0.5413 (difference -0.0250, SE 0.0028).
  - Round 2 (`fit-round2.json`): the held-out gain is 0.0155, against 0.0177 in round 1.
  - Tests: `tune_ext::tests::recording_and_fit_do_not_depend_on_the_worker_count` and `synthetic_weights_are_recovered`.
- **Ablation:** the placebo scored 215/400 (0.537, [0.488, 0.586]) on these deals.
  - Individual (dev 20700100): 193/400 (0.482, [0.434, 0.531]), -0.035 [-0.104, +0.035], discordant 22/29, 9424 vs 9864 / 114184 vs 118881.
  - High budget (dev 20700200): 188/400 (0.470, [0.422, 0.519]), -0.060 [-0.135, +0.017], 25/37, 31388 vs 32113 / 363551 vs 368678.
  - Sign-constrained: 198/400 (0.495, [0.446, 0.544]), -0.010 [-0.085, +0.065], 29/31.
  - Files: `docs/port/eval/P6-G/rows/`.
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-35 — The evaluation is linear; interaction terms and other candidate features gained nothing
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:208-212`, `docs/python/AI.md:219-221`, `docs/python/AI.md:475-477`, `src/gcg_sim/ai/evaluation.py:303-307`
- **Quotes:** “Interaction terms (life × enemy threat), a sharper lethal estimate, a held-trick feature and dropping `bursts` were measured and gained nothing (within ±0.001).”; “**Linear evaluation.** The model is still linear.”
- **What is weak:** one logistic model for every phase of the game; base-set features that did not help were dropped (Unit count, Blocker count, EX Resources, deck size, exposed flag, maximum AP, trash size, a continuous attack margin); interaction terms gained nothing. The role features replaced the forced antisymmetry, the one measured misspecification.
- **Resolution plan:** strengthened evaluation: phase-conditioned linear model (separate weight vectors for early, mid and late game, binned by turn and level, sharing the role features), fitted with the W-34 tool; the exact threat solver (W-38) replaces the approximate lethal terms. Evaluation sums run in a fixed order in f64. Fixed order alone does not make results identical across platforms, because the logistic leaf calls `exp` (`evaluation.py:303-307`) and platform libm results can differ (W-137). So the strengthened evaluation calls the port's portable `exp` and `ln`, never the platform libm. W-137 names that implementation: safe Rust in `gcg-core` that uses only IEEE-754 basic operations. The reference keeps libm and is pinned on the golden platform only.
- **Verification** (ablation + unit): out-of-fold log-loss against the single linear refit; ablation row; unit test that strengthened evaluation and leaf values match recorded vectors bit for bit on two platforms (CI matrix: macOS arm64 and Linux x86-64).
- **Status:** resolved by measurement (not adopted): `phase_weights` is a phase-conditioned linear model: one weight vector per turn tercile of the training positions, turns 3-6, 7-12 and 13 on. Its sums are in fixed order, and it uses portable math with `portable_math`. It measured negative at the high budget and is off in both presets. Deviation: the bins are by turn only, not by turn and level.
- **Evidence:**
  - Log-loss out of fold: `phase_weights` 0.5297 against 0.5338 for the single refit; `determinized_fit+phase_weights` 0.5275 against 0.5318; on the unseen `fast` positions, 0.5316 against 0.5373.
  - Unit tests `w35_leaf_values_match_the_recorded_vectors` (22 models, 20 positions, bit for bit) and `tune_ext::tests::synthetic_weights_are_recovered` pass.
- **Ablation:**
  - Individual (dev 20700100): 200/400 (0.500, [0.451, 0.549]), +0.000 [-0.073, +0.073], discordant 28/28, 9519 vs 9802 / 115476 vs 117949.
  - High budget (dev 20700200): 188/400 (0.470, [0.422, 0.519]), -0.060 [-0.128, +0.009], 19/31, 30994 vs 32181 / 358286 vs 369310.
  - The P6-G bundle (with `phase_weights`) confirmation (confirm 30700100): 821/1600 (0.513, [0.489, 0.538]), +0.026 [-0.011, +0.063], 125/104, 9725 vs 9850 / 118012 vs 119270 (`docs/port/eval/P6-G/rows/confirm-p6g_bundle-1600.json`).
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-36 — The evaluation was trained on three tuning decks and transfers weakly to other decks
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/python/AI.md:213-218`, `docs/python/AI.md:228-231`, `docs/python/AI.md:329-332`
- **Quotes:** “+0.0030 (SE 0.003, slightly worse) on example-deck games”
- **What is weak:** tuning decks are Zeon green/red, Wing green/blue and Tekkadan purple; role weights gain −0.0090 nats out of fold and −0.0074 on test decks but are slightly worse (+0.0030) on example decks; at the tuning budget a role agent scored 384/800 = 0.480 [0.446, 0.515] against a base refit.
- **Resolution plan:** the W-34 refit trains on the tuning half of the deck panel (W-20) and reports per-deck held-out log-loss; a per-deck regression gate.
- **Verification** (e2e): per-deck held-out log-loss table: no example or held-out panel deck worse than `ROLE_WEIGHTS` by more than one standard error.
- **Status:** resolved by measurement (not adopted): the refit trains on the panel's tuning half plus the Python tuning decks. It reports the per-deck out-of-fold and leave-one-deck-out tables, which form the per-deck regression gate. The gate holds for the three example decks. It fails for the tuning deck red-purple-control (+0.032 to +0.052 nats, 8-12 SEs, under every refit) and narrowly for tuning-zeon. No refit is adopted: `refit_weights` measured negative at game level (W-34), so the shipped presets keep `ROLE_WEIGHTS`. The held-out panel decks, `blue-red-tempo` and `green-white-link`, are not scored, by the decision recorded in `docs/port/eval/weakness-status/GAP-A2.md`: the per-deck gate is moot while no refit ships. The tuning, dev and confirm roles refuse held-out pairs (PLAN §12.5; GAP-A2 checked that `gcg-eval tune-leaf --pairs held-out --role confirm` exits 1), and using held-out positions to choose a fit would make them a screening step. If a refit is ever proposed for adoption, the lead records leaf positions on the 13 held-out pairs after the final evaluation under a fresh eval master (`gcg-eval tune-leaf --decks panel --pairs held-out --role eval --record FILE`) and scores the models with `--test-positions FILE` first.
- **Evidence:** `docs/port/eval/P6-G/fit-deck-transfer.json`, leave-one-deck-out, in nats against `ROLE_WEIGHTS`:
  - blue-white-federation -0.032 to -0.001, purple-white-tekkadan -0.015 to -0.005, red-green-zeon -0.027 to -0.015;
  - blue-green-ramp and purple-blue-swarm better by 0.011 to 0.045;
  - tuning-wing and tuning-tekkadan within one SE;
  - tuning-zeon +0.004 to +0.008;
  - red-purple-control +0.032 to +0.052.

  Folds by pair: `fit-transfer.json`.
- **Ablation:** as W-34 (`refit_weights`).

### W-37 — The evaluation ignores most keywords, card text and effects
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:185-200`, `.claude/skills/gcg-refresh-data/references/engine-extension.md:21`, `src/gcg_sim/ai/evaluation.py:30-45`, `src/gcg_sim/ai/evaluation.py:132-140`, `src/gcg_sim/ai/evaluation.py:159-202`, `src/gcg_sim/ai/evaluation.py:246-269`
- **Quotes:** “give the keyword a value in the evaluation features, so search does not ignore it”; “hand[_HAND_SLOT.get(cd.card_type, 3)] += 1”
- **What is weak:** features are life, stats, linked and ready counts, hand counts by type, level and burst density; the only keywords read are Blocker and High-Maneuver/can't-be-blocked; Breach, Repair, Support, First Strike, Suppression and printed abilities have no feature; unknown card types fall into the Base hand slot; burst density is one fraction over all unseen cards. Each new keyword needs a manual evaluation update.
- **Resolution plan:** strengthened evaluation adds per-keyword amount features on the battlefield (one per `Kw` variant, derived through an exhaustive `match`, so a new keyword cannot be forgotten), counts of Units with activatable and triggered abilities from the IR, an explicit hand slot per card type (unknown type is an error), and per-Shield Burst probability. Fitted with W-34.
- **Verification** (unit + ablation): unit test: every `Kw` variant maps to a feature; log-loss improvement; ablation row.
- **Status:** resolved by measurement (not adopted): four feature groups, each one flag and fitted before use. None is kept; all are off in both presets.
  - `keyword_features`: per `Kw` variant, exhaustive `match`.
  - `ability_count_features`.
  - `hand_slots`: per card type, exhaustive `match`.
  - `burst_shield_probability`.
- **Evidence:**
  - Unit tests `w37_every_keyword_has_a_feature` and `w37_every_hand_card_type_has_a_slot` pass.
  - Log-loss out of fold, against 0.5338 for `refit_weights`: keywords 0.5306, abilities 0.5338, hand slots 0.5327, Burst 0.5327. All four with the determinised fit give 0.5264, and with phases 0.5176, the best model (held-out pair 0.5680 against 0.5857 for `ROLE_WEIGHTS`).
- **Ablation:** files in `docs/port/eval/P6-G/rows/`.
  - Individual (dev 20700100):
    - keywords: 194/400 (0.485, [0.436, 0.534]), -0.030 [-0.100, +0.040], 23/29.
    - abilities: 191/400 (0.477, [0.429, 0.526]), -0.045 [-0.116, +0.027], 23/32.
    - hand slots: 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.041, +0.110], 34/27.
    - Burst: 193/400 (0.482, [0.434, 0.531]), -0.035 [-0.116, +0.047], 32/39.
  - Hand-slot confirmation (confirm 30700100): 752/1600 (0.470, [0.446, 0.494]), -0.060 [-0.097, -0.022], 95/143. This is negative.
  - High budget (dev 20700200):
    - keywords: 184/400 (0.460), -0.080 [-0.159, +0.000], 26/42.
    - abilities: 187/400 (0.468), -0.065 [-0.129, +0.000], 16/29.
    - hand slots: 185/400 (0.463), -0.075 [-0.145, -0.003], 20/35.
    - Burst: 194/400 (0.485), -0.030 [-0.101, +0.042], 24/30.
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-38 — can_kill and race features are crude board heuristics
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:174-180`, `docs/python/AI.md:190`, `docs/PROGRESS.md:113-114`, `src/gcg_sim/ai/evaluation.py:59`, `src/gcg_sim/ai/evaluation.py:173-176`, `src/gcg_sim/ai/evaluation.py:205-212`, `src/gcg_sim/ai/evaluation.py:244-277`, `docs/port/inventory/08-ai-policy-eval.md:1319-1322`
- **Quotes:** “can_kill leaves (10.8%) are converted by rollouts only 46% of the time”; “castable removal in hand”; “capped at :data:`RACE_CAP`”; “It counts the defender's Blockers whether rested or not”
- **What is weak:** can_kill counts all ready Units (all Units for the non-active side) as attackers whatever their AP, CANT_ATTACK or summoning state, counts the Base as one hit whatever its HP, and lets any castable removal answer one Blocker. The race assumes every Unit with AP attacks each turn, each Blocker stops the largest blockable attack and the Base absorbs hits at the mean connecting AP, with no development, removal or recovery, capped at 6 turns. It counts every defender Blocker, rested ones included (`evaluation.py:249-253`), while the per-side Blocker feature counts only active ones (`evaluation.py:173-176`). This asymmetry is undocumented (W-32).
- **Resolution plan:** strengthened evaluation replaces can_kill with the exact bounded lethal solver (W-17/W-03) on the determinized board. It computes the race with engine-correct attackers and blockers (W-32), tracking which Blockers are active on each simulated turn, and the race cap becomes a fitted feature transform. The reference keeps `_race` as written, rested Blockers included.
- **Verification** (unit + ablation): unit test on 10,000 positions from random and search games: the strengthened can_kill agrees with exhaustive search of attack lines (reference disagreement rate recorded). A second unit test uses a position with a rested defender Blocker: the reference race counts it (pinned), and the strengthened race does not count it on a turn when it is still rested. Log-loss and ablation row.
- **Status:** resolved by measurement (not adopted): two leaf features, both implemented, unit-tested and measured null or negative under `ROLE_WEIGHTS`, and both off in both presets. Their own weights would come from a refit, and P6-G's refit measured negative (W-34).
  - `solver_can_kill`: the exact static attack step as `can_kill`.
  - `effective_blockers`: the engine-correct count rule and race.
- **Evidence:**
  - The `can_kill` census (`mech::p6b::can_kill_census`, feature `slow`, dev 20200300) used 10,000 turn-player MAIN positions from 231 random and greedy games, 1,403 of them lethal. The static attack step disagrees with an exhaustive search on 61 (0.61%), the reference `can_kill` on 694 (6.94%).
  - Unit test `the_strengthened_race_skips_a_blocker_that_stays_rested_this_turn` (reference pinned).
- **Ablation:**
  - `solver_can_kill` individual (dev 20200100): 203/400 (0.507, [0.459, 0.556]), +0.015 [-0.053, +0.083], discordant 26/23, 9683 vs 9718 / 116037 vs 116349, vs placebo -0.0200 [-0.0553, +0.0153].
  - High budget: 196/400 (0.490, [0.441, 0.539]), -0.020 [-0.096, +0.057], 29/33, 32063 vs 32600 / 370883 vs 379262.
  - `effective_blockers` as W-32.
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-39 — The negative tempo weight is an artifact of when positions are recorded
- **Category:** ai-evaluation · **Resolution type:** port-preserve + AI (strengthened search)
- **Sources:** `docs/python/AI.md:187`, `docs/python/AI.md:202-204`
- **Quotes:** “The negative `tempo` weight reflects when positions are recorded”
- **What is weak:** the base set's `tempo` weight is −0.20 because positions are recorded after the turn player's draw and resource placement, which hand and level already count; the roles set uses an intercept instead.
- **Resolution plan:** the reference keeps `TUNED_WEIGHTS` for `baseline-standard` bit-exactly. Strengthened feature sets have no tempo feature (intercept only), and the Rust tuning tool records and reports the exact recording point so the fit and the leaf point cannot drift apart.
- **Verification** (unit): unit test: strengthened feature sets contain no tempo feature, and the tuning tool's recording point equals the search's leaf point (both asserted from the same constant).
- **Status:** resolved: no strengthened feature is a tempo indicator. The role block's constant is named `intercept`, and the recorder and the search share one constant, `p6g::LEAF_POINT`. The reference keeps `TUNED_WEIGHTS` for `baseline-standard` bit for bit. The optional `no_tempo_feature` measured negative and is not adopted.
- **Evidence:**
  - Unit tests `w39_no_tempo_feature` and `w39_search_leaves_are_at_the_recording_point` (six traced searches, over 1,000 leaves, asserted from the same constant) pass.
  - The fitted intercept at the leaf point is -0.47, against -0.75 in `ROLE_WEIGHTS`.
- **Ablation:**
  - `no_tempo_feature` individual (dev 20700100): 196/400 (0.490, [0.441, 0.539]), -0.020 [-0.097, +0.058], discordant 30/34, 9676 vs 9832 / 116858 vs 117928.
  - High budget: 197/400 (0.492, [0.444, 0.541]), -0.015 [-0.081, +0.052], 22/25, 30520 vs 31875 / 353029 vs 363165.

### W-40 — Weight-tuning methodology limits
- **Category:** ai-evaluation · **Resolution type:** eval tooling
- **Sources:** `src/gcg_sim/ai/tuning.py:1-16`, `src/gcg_sim/ai/tuning.py:50-62`, `src/gcg_sim/ai/tuning.py:64-96`, `src/gcg_sim/ai/tuning.py:148-162`, `src/gcg_sim/ai/tuning.py:184-213`, `src/gcg_sim/ai/tuning.py:280-289`, `src/gcg_sim/ai/tuning.py:341`, `src/gcg_sim/ai/tuning.py:348-364`, `src/gcg_sim/ai/tuning.py:395-400`, `src/gcg_sim/ai/compare.py:4-5`, `docs/python/AI.md:252-253`, `ef05730`
- **Quotes:** “went on to win (draws are dropped)”; “Challenger's score in one game (1 win, 0.5 draw, 0 loss).”; “wins-only strength metric”
- **What is weak:** three decks (three of nine matchups are mirrors); the label is the game outcome at one position per turn; correlated positions; even/odd game split; the final weights are fitted on all positions without a held-out check; λ fixed at 1 with 30 Newton steps in pure Python; weights rounded to 4 decimals; self-play uses the same weights on both sides; validation always plays against `HAND_WEIGHTS` (tuned 112 vs hand 104, Wilson [0.452, 0.584]), scores draws as 0.5 (inconsistent with the wins-only metric) and reports no interval.
- **Resolution plan:** the gcg-eval `tune` subcommand: game-clustered k-fold cross-validation, λ chosen by CV, final weights checked on a held-out deck set, validation against the current default weights with the wins-only metric, Wilson and paired intervals (W-42), the panel decks of W-20, tuning budget equal to the target preset's, deterministic and independent of worker count.
- **Verification** (unit): same seed gives bit-identical weights with 1, 4 and 10 workers; no CV fold splits a game; validation counts draws as non-wins; a synthetic dataset with known weights is recovered within tolerance.
- **Status:** resolved: `gcg-eval tune` (`crates/gcg-eval/src/tune.rs`) fits with the target agent's own budget on the panel's tuning pairs. It uses game-clustered k-fold CV, lambda by CV, unrounded weights, held-out pairs as the check, and `--validate N` (a wins-only row).
- **Evidence:**
  - Unit tests `tune::tests::w40_synthetic_weights_are_recovered`, `w40_no_fold_splits_a_game`, `solve_matches_a_known_system` and `positions_round_trip`.
  - `crates/gcg-eval/tests/phase6.rs::w40_tune_is_deterministic_and_refits_recorded_positions`: identical bytes at 1 and 4 workers. The 1/4/10-worker identity of the games is W-150's.
- **Ablation:** no AI effect (row I).

### W-41 — Matchup estimates are not calibrated against real tournament results
- **Category:** ai-evaluation · **Resolution type:** eval tooling (data from the user)
- **Sources:** `docs/python/AI.md:506`, `docs/ASSUMPTIONS.md:88-89`
- **Quotes:** “Calibrate matchup estimates against real tournament results (needs data from the user).”; “No real-world win rates were fetched or invented”
- **What is weak:** strength is measured only by head-to-head play, puzzles, the audit and regret; no real results exist in the repo, and calibrating needs data only the user can supply.
- **Resolution plan:** gcg-eval `calibrate`: reads a user-supplied file of observed matches (two decklists, wins, losses, draws, source, date), simulates each matchup with a chosen preset, and reports simulated vs observed rates with intervals, Brier score, log-loss and reliability bins. Nothing is fetched or invented; the tool ships with a schema and a synthetic fixture. Supplying real data is a question for the user, raised by the lead.
- **Verification** (e2e): synthetic fixture whose "observed" rates equal simulated rates gives zero calibration error; a fixture with a known bias reports it; schema validation rejects malformed input.
- **Status:** resolved by its verification (PLAN §12.10): `gcg-eval calibrate` with the embedded schema `crates/gcg-eval/schema/observed-matches.v1.schema.json` and a synthetic fixture. Follow-up: calibrating against real tournament results, once the user supplies data (none was supplied, A32 item 5).
- **Evidence:**
  - `crates/gcg-eval/tests/phase6.rs::w41_calibrate_on_synthetic_fixtures`: equal counts give ECE 0 and bias 0; a shift of +2 wins per matchup gives a bias above 0.1; malformed entries are refused.
  - `calibrate::tests::equal_counts_have_zero_error_and_a_shift_shows_as_bias` and `the_schema_compiles_and_rejects_malformed_entries`.
- **Ablation:** no AI effect (row I).

### W-42 — No paired analysis of same-seed runs; compare reports a pooled Wilson interval
- **Category:** reporting · **Resolution type:** eval tooling
- **Sources:** `docs/python/AI.md:516-517`, `.claude/skills/gcg-benchmark/SKILL.md:104-110`, `src/gcg_sim/ai/compare.py:1-5`, `src/gcg_sim/ai/compare.py:11-12`, `src/gcg_sim/ai/compare.py:146-178`, `src/gcg_sim/ai/selfplay.py:88-110`, `bench/__pycache__/weakness.cpython-315.pyc`, `bench/__pycache__/stats.cpython-315.pyc`
- **Quotes:** “Paired analysis of same-seed variant runs in the benchmark skill (fewer games for the same precision).”; “the win rate carries a Wilson 95% interval”; “game_seed = derive_seed(master_seed, "block", block, "decks", swap_decks)”
- **What is weak:** a mirrored block of four holds two deals, not one. `mirrored_specs` derives the game seed from the block and the deck order (`swap_decks`), and each deal is played twice with the agents swapped between seats, so the challenger plays both decks and both seats within each deal. The `mirrored_specs` docstring and `docs/python/AI.md:326-327` say one deal per block of four (W-129). The games of a deal share their cards, yet `compare.py` treats all games as independent and gives no paired statistic. Variants also share per-decision seeds through the config name. The benchmark skill compares card swaps by unpaired Wilson intervals ("if the intervals overlap heavily"). The lost harness used paired Newcombe and block-bootstrap intervals.
- **Resolution plan:** gcg-eval paired statistics for every head-to-head and ablation row: per-deal paired differences (each deal is the pair of games with the same game seed, played with the agents swapped), Newcombe interval for paired proportions, McNemar discordant counts and a seeded bootstrap over deals. Also a `compare-runs` command that pairs two same-seed `gcg-sim benchmark` result directories game by game (additive output; the reports themselves are unchanged).
- **Verification** (unit + e2e): unit tests against published Newcombe test vectors. Unit test: the pairing groups exactly the two games of each block that share a game seed, giving two pairs per block of four. e2e: two identical same-seed runs give a zero difference with no discordant pairs, and a known-better variant gives a narrower paired interval than the unpaired one.
- **Status:** resolved: every head-to-head and row block pairs the two games of each deal. It reports the 2x2 table, the McNemar counts, Newcombe's interval and a seeded bootstrap over deals. `gcg-eval compare-runs` pairs two same-seed benchmark runs.
- **Evidence:**
  - `stats::tests::w42_newcombe_published_example` (36/12/2/0 gives 0.0569 to 0.3404, cross-checked by `crates/gcg-eval/tests/vectors.rs::newcombe_matches_the_python_reference`; T-080).
  - `w42_bootstrap_over_deals`.
  - `t149_seat0_balances_play_and_draw_in_every_block`.
  - `crates/gcg-eval/tests/phase6.rs::w42_compare_runs_pairs_same_seed_runs`.
- **Ablation:** no AI effect (row I).

### W-43 — Decision-level regret (oracle replay) tooling was never committed
- **Category:** tooling · **Resolution type:** eval tooling
- **Sources:** `docs/python/AI.md:514-515`, `docs/PROGRESS.md:109-112`, `scripts/blind_transcripts.py:1-17`
- **Quotes:** “Decision-level regret tooling (the oracle replay used in the investigation) as a committed tool for future search changes.”
- **What is weak:** every regret figure in `docs/python/AI.md` (target extension, budget scaling, policy targeting) came from investigation tooling that is not in the repo; `scripts/blind_transcripts.py` covers only the audit transcripts.
- **Resolution plan:** gcg-eval `regret`: sample positions from recorded games deterministically, run the candidate search and an oracle (strengthened search at 8x budget), play both chosen moves out in M shared worlds, and report regret per decision with a paired standard error by decision kind. Deterministic and worker-count independent.
- **Verification** (unit + e2e): the oracle's regret against itself is exactly 0; an agent that picks random targets shows positive target regret with an interval excluding 0; identical output for 1 and 8 workers.
- **Status:** resolved: `gcg-eval regret` (`crates/gcg-eval/src/regret.rs`). The P6-D, P6-F and P6-H rows used it.
- **Evidence:**
  - `crates/gcg-eval/tests/phase6.rs::w43_regret_of_an_agent_against_itself_is_zero`: 12 positions, regret exactly 0, identical at 1 and 4 workers.
  - `w43_random_targets_show_positive_target_regret`: positive with a 95% interval above 0; a probe run gave 0.0183 [0.0078, 0.0289].
- **Ablation:** no AI effect (row I).

## AI setup

### W-44 — Play/draw choice: policy-vs-policy simulations, biased to going first, capped simulations scored 0.5
- **Category:** ai-setup · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:131-136`, `docs/python/AI.md:163`, `docs/PROGRESS.md:69-70`, `src/gcg_sim/ai/setup.py:86-120`
- **Quotes:** “the agent goes second only when that is better by more than two standard errors of the paired difference”; “go second only if that is significantly better; otherwise go first.”
- **What is weak:** `first_player_games` pairs (8 fast, 24 standard, 48 strong) of policy-vs-policy games; goes second only when 2 SE better; simulations capped at 3,000 decisions, or drawn, silently score 0.5. Player One won 436/720 in MCTS self-play; policy and greedy always go first.
- **Resolution plan:** strengthened search uses its calibrated rollout policy (W-23) in the simulations, excludes capped or drawn simulations from the paired estimate instead of scoring them 0.5, and records the number excluded in the trace.
- **Verification** (unit + e2e): unit test: a capped simulation is excluded (strengthened) and scored 0.5 (reference, pinned); e2e: the trace reports go-first rates per matchup and excluded simulations.
- **Status:** resolved: `exclude_capped_play_draw` is adopted in both shipped presets as a zero-cost pre-game guard (M15). A pair with a capped, drawn or failed game is excluded from the paired estimate. Each choice's reported mean scores such a game by the evaluation instead of 0.5. With every game finished, it equals the reference bit for bit (`docs/port/eval/weakness-status/P6-H.md`).
- **Evidence:**
  - Unit tests in `mech::p6h` pass: `unfinished_games_are_excluded_from_the_pairs_and_scored_by_the_evaluation`, `capped_games_are_excluded_from_the_pairs_and_scored_by_the_evaluation`, `drawn_games_are_excluded_from_the_pairs_and_the_reference_scores_them_even` (reference pinned at 0.5), `play_draw_is_the_reference_when_every_game_finishes` and `pregame_choices_are_identical_for_any_worker_count`.
  - `crates/gcg-search/tests/scenarios/p6h_pregame.rs::w44_unfinished_play_draw_games_leave_the_paired_estimate`: 13 of 48 pairs have a drawn game. The reference reports all 48 at 0.5; the mechanism counts 35.
  - e2e trace (`docs/port/eval/P6-H/w44-e2e-head-to-head-dev.json`, `w44-e2e-trace-summary-dev.json`): each agent chose in 30 games and went first in all 30; 720 of 720 pairs were counted.
- **Ablation:**
  - Individual with `--choose-first agents` (dev 20800044): 212/400 (0.530, [0.481, 0.578]), +0.060 [-0.007, 0.126], discordant 30/18, 9,917.6 vs 9,658.0 / 117,992.4 vs 115,571.7. It is identical game for game to the null control, 0 of 400 games changed (`docs/port/eval/P6-H/w44-*-dev.json`).
  - Leave-one-out from `B_core` with `--choose-first agents` (dev 21500250): 216/400 (0.540, [0.491, 0.588]), +0.080 [+0.002, +0.156], 40/24, 32581 vs 31934 / 611051 vs 582693. It is identical to the protocol's placebo, 0 of 400 games changed (`docs/port/eval/M15/rows/dev-loo-agents-exclude_capped_play_draw-400.json`).
  - Pre-game simulations' applies are not counted in applies per game (P6-H interface note 2).

### W-45 — Mulligan heuristic: greedy four-turn opening score with a fixed 90% threshold
- **Category:** ai-setup · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/python/AI.md:137-143`, `docs/python/AI.md:163`, `src/gcg_sim/ai/setup.py:22-27`, `src/gcg_sim/ai/setup.py:30-60`, `src/gcg_sim/ai/setup.py:63-83`, `bench/__pycache__/weakness.cpython-315.pyc`
- **Quotes:** “The agent redraws when the hand scores below 90% of the average of `mulligan_samples` fresh five-card hands”; “How much board a hand develops over the first turns if played out greedily.”
- **What is weak:** `opening_score` counts only Units and Pilots over four turns with a greedy curve, ignoring Commands, Bases, effects and the matchup; `KEEP_RATIO = 0.9` is fixed; `mulligan_samples` 96 (192 for strong); policy and greedy rollouts always keep (WK6 "mulligan regret" in the lost benchmark).
- **Resolution plan:** strengthened mulligan by paired simulation, reusing the play/draw machinery: keep vs redraw compared on shared determinized opening worlds with the strengthened rollout policy; redraw only when better by 2 paired SE, otherwise fall back to the opening-score rule.
- **Verification** (scenario + ablation): scenarios: a hand of early Commands and a Base that `opening_score` undervalues, and a Unit-only hand that fails against the matchup; ground truth from 2,000 paired simulations; reference wrong (recorded), strengthened right in at least 14/16 seeds. Mulligan regret via the W-43 tool; ablation row.
- **Status:** resolved by measurement (not adopted): `paired_mulligan` plays KEEP and REDRAW on shared worlds for four turns each. It decides by two paired standard errors and falls back to the reference rule otherwise. It is scenario-tested and measured non-negative on dev and confirm, game by game against the null control. It is off in both shipped presets (`crates/gcg-search/src/presets.rs`), and no M15 bundle row included it. GAP-A2 records the reason: its individual dev row is negative, -0.060 [-0.137, +0.018], and the adoption rule keeps only mechanisms with a measured non-negative effect, so it was not carried into the M15 bundles (`docs/port/eval/weakness-status/GAP-A2.md`).
- **Evidence:**
  - Scenario test W-45 (T-122; `crates/gcg-search/tests/scenarios/p6h_pregame.rs`, pass):
    - `w45_the_reference_redraws_a_ramp_hand_that_wins_more_kept`: the truth is -0.1405 (se 0.0144), keep. The reference is right 0/16 at `standard` and at `strong`; `paired_mulligan` 16/16 at both.
    - `w45_the_reference_keeps_a_hand_that_wins_more_redrawn`: the truth is +0.1610 (se 0.0135), redraw. The reference is right 0/16 and 0/16; `paired_mulligan` 16/16 and 16/16.
  - Mulligan regret (W-43 tool, 120 REDRAW positions, dev 20800245): the reference rule differs from the oracle on 27/120, mean 0.0173 [0.0079, 0.0267]; `paired_mulligan` on 15/120, mean 0.0046 [-0.0004, 0.0095].
  - Cost: about 6,100 applies per REDRAW decision at the reference `standard` budget and 12,200 at `strong`. They are not counted in applies per game.
- **Ablation:** files in `docs/port/eval/P6-H/w45-*.json`.
  - Dev (seed 20800045):
    - individual: 188/400 (0.470, [0.422, 0.519]), -0.060 [-0.137, 0.018], discordant 26/38, 10,006.2 vs 9,915.5 / 120,761.1 vs 119,326.7.
    - null control: 181/400 (0.453), -0.095 [-0.168, -0.021].
    - against the control, game by game: +0.0175 [-0.0049, 0.0399], 60 games changed, 14/7.
  - Confirm (seed 30800045, run once):
    - individual: 836/1600 (0.522, [0.498, 0.547]), +0.045 [0.009, 0.081], 128/92, 9,884.0 vs 9,907.1 / 119,036.2 vs 119,757.9.
    - control: 822/1600 (0.514), +0.028 [-0.007, 0.062].
    - against the control, game by game: +0.0088 [-0.0009, 0.0184], 173 games changed, 38/24.
  - Leave-one-out: not applicable (not in the shipped bundle).

## Engine

### W-46 — Safety caps: 200 turns and 20,000 decisions share one end reason; the decision cap is untested; self-play stops earlier with no end reason
- **Category:** engine · **Resolution type:** Rust fix (additive diagnostics)
- **Sources:** `README.md:165-166`, `docs/ASSUMPTIONS.md:64`, `docs/REPORTS.md:40-45`, `docs/INTERFACES.md:18`, `src/gcg_sim/engine/game.py:33`, `src/gcg_sim/engine/game.py:112-113`, `src/gcg_sim/engine/game.py:325-332`, `src/gcg_sim/engine/game.py:737-742`, `src/gcg_sim/engine/game.py:786`, `src/gcg_sim/engine/state.py:507-508`, `src/gcg_sim/engine/types.py:155`, `src/gcg_sim/runner/config.py:15`, `src/gcg_sim/runner/config.py:31`, `src/gcg_sim/runner/game.py:44`, `src/gcg_sim/runner/match.py:22-26`, `src/gcg_sim/reports/results.py:35-37`, `src/gcg_sim/ai/selfplay.py:22`, `src/gcg_sim/ai/selfplay.py:60-85`, `src/gcg_sim/ai/tuning.py:159-161`, `src/gcg_sim/ai/tuning.py:286-289`, `src/gcg_sim/tools/fuzz.py:107-110`, `tests/runner/test_match.py:158-163`
- **Quotes:** “Games that reach the engine's safety cap (200 turns or 20,000 decisions) are reported as `turn_limit` draws.”; “TURN_LIMIT = "turn_limit" # engine safety cap (reported, never silent)”; “if st.action_count > st.max_actions:”; “winner = -1 if st.winner is None else st.winner”
- **What is weak:** both caps end the game with `EndReason.TURN_LIMIT`, so reports cannot tell them apart; `action_count` grows only in `apply`; only the turn cap is tested (`tests/runner/test_match.py:158-163`). A turn-limit game scores no match point and the next chooser is re-rolled; the fuzz tool counts it as a failure. Self-play (`compare`, strength tests, tuning) stops at its own `DECISION_LIMIT = 20000`, before the engine's `> 20000` check, leaving `winner = None`: `play()` maps it to a draw, tuning drops it, `duel` scores 0.5, and no end reason is recorded.
- **Resolution plan:** the Rust engine keeps the `turn_limit` end reason in every reference-equivalent output, and records which cap fired (`CapKind::Turns` or `CapKind::Decisions`) in the game record's diagnostics (exposed in gcg-eval traces and a `--diagnostics` side file, not in `results.json` or replays). gcg-eval self-play has no loop cap of its own: it relies on the engine caps and records the end reason; capped games are listed in every head-to-head summary.
- **Verification** (unit + e2e): unit tests: `max_actions = 10` ends with `turn_limit` and `CapKind::Decisions`; `turn_limit = 2` gives `CapKind::Turns`; equivalence of reports with Python for both. e2e: a gcg-eval head-to-head with a tiny decision cap lists the capped games instead of silent draws.
- **Status:** resolved: every reference-equivalent output keeps `turn_limit`. `GameState::cap_kind()` (`crates/gcg-engine/src/diag.rs`) names the cap that fired. It needs no new state: the decision cap can fire only when `action_count > max_actions`. The cap kind is written to the opt-in `diagnostics.json` (P6-M). Head-to-head reports list the games the decision limit stopped (`unfinished`, `crates/gcg-eval/src/head_to_head.rs`).
- **Option location (A30):** `gcg-eval benchmark --diagnostics`. `gcg-sim`, `results.json` and replays are unchanged.
- **Evidence:**
  - `crates/gcg-engine/tests/p6j_engine.rs::w46_both_caps_report_turn_limit_and_name_the_cap`: `max_actions = 10` gives `turn_limit` and `CapKind::Decisions`; `turn_limit = 2` gives `CapKind::Turns`. Each final digest equals Python's, and a reloaded snapshot keeps the kind.
  - `w46_other_endings_have_no_cap_kind`.
  - `crates/gcg-runner/tests/p6m_analysis.rs::w46_the_cap_kind_names_the_cap_that_fired`.
  - `crates/gcg-eval/tests/head_to_head.rs::the_report_has_pooled_held_out_and_per_pair_blocks`: the `unfinished` list, on recorded outcomes.
  - No fuzz game hits a cap (`crates/gcg-diff/tests/p6j_runs.rs`).
- **Ablation:** no AI effect (row J). The AI never calls `cap_kind`.

### W-47 — No guard against decision-free loops inside advance()
- **Category:** engine · **Resolution type:** Rust fix (error path only)
- **Sources:** `src/gcg_sim/engine/game.py:734-760`, `src/gcg_sim/engine/game.py:786`
- **Quotes:** “while st.pending is None and st.winner is None:”
- **What is weak:** `advance()` checks only the `max_actions` cap and `action_count` grows only when a decision is applied, so a chain of triggers or frames that never asks for a decision (e.g. delayed triggers re-triggering each other) would hang. No known card does this.
- **Resolution plan:** Rust `advance()` counts engine steps since the last decision and returns `EngineError::NoProgress` past a large bound (e.g. 1,000,000 steps). Legal games never get near it, so every output is unchanged; the fuzzer treats the error as a crash.
- **Verification** (unit + fuzz): a test-only card script with two mutually re-triggering delayed triggers returns `NoProgress` within the bound; 10,000 fuzz games never hit it.
- **Status:** resolved: `advance` returns `EngineError::NoProgress` after `NO_PROGRESS_STEPS` (1,000,000) loop iterations without a decision (`crates/gcg-engine/src/game.rs`). Legal games never come near it.
- **Evidence:**
  - `crates/gcg-engine/tests/p6j_engine.rs::w47_mutually_retriggering_delayed_triggers_end_in_no_progress`: Python loops forever; Rust returns `NoProgress` with `action_count` 1.
  - `l5_procedure.rs::advance_without_progress_fails_instead_of_looping`.
  - Fuzz: `crates/gcg-diff/tests/p6j_runs.rs::p6j_fuzz_diagnostics_on_all_10000_python_fuzz_games` (features `slow`, `paranoid`). All 10,000 games of Python's `fuzz --games 10000 --seed 2026` replay to Python's per-game rows (856,034 actions); none returned `NoProgress`.
- **Ablation:** no AI effect (row J). Error path only.

### W-48 — The derived-view fixpoint is silently capped at six passes
- **Category:** engine · **Resolution type:** Rust fix (diagnostic, identical output)
- **Sources:** `src/gcg_sim/engine/view.py:5`, `src/gcg_sim/engine/view.py:409-461`, `tests/perf/__pycache__/test_viewdeps.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “for _ in range(6):”; “GD02-076 (AP 4) gains <Blocker> while it has 5 or more AP; a lasting AP+1 is applied in”
- **What is weak:** constant and lasting effects are applied in a small fixpoint; if the view has not converged after 6 passes the last iteration is used with no error, and layers are not ordered by dependency or timestamp. The lost view-dependency tests record a case that needs a confirming pass (GD02-076 gains <Blocker> only after a lasting AP+1), and custom steps that run `Apply` through `interp.execute` are opaque to static analysis.
- **Resolution plan:** Rust keeps the same six-pass loop (identical results) and adds a convergence flag: a view that hits the cap without converging raises a diagnostic in the `paranoid` feature and in fuzz runs. Any future dependency-ordered view (W-124) must reproduce the fixpoint result, including the GD02-076 case.
- **Verification** (unit + fuzz): unit test: GD02-076 with a lasting AP+1 has AP 5 and <Blocker>; a test-only oscillating script triggers the diagnostic; 10,000 fuzz games report zero non-converged views.
- **Status:** resolved: the six-pass fixpoint is unchanged. A view that stops at the cap without converging is counted in the per-thread `gcg_engine::diag::non_converged_views()`, which fuzz and paranoid runs read.
- **Evidence:**
  - `crates/gcg-engine/tests/p6j_engine.rs::w48_gd02_076_with_a_lasting_ap_plus_one_gains_blocker`: AP 5 and <Blocker>, converged, as in Python.
  - `w48_an_oscillating_constant_is_reported`: `converged() == false` and the counter is incremented.
  - `diag::tests::the_counter_belongs_to_its_thread`.
  - The 10,000-game fuzz replay of W-47: 0 non-converged views.
- **Ablation:** no AI effect (row J).

### W-49 — The derived-view cache relies on manual touch() calls and a heuristic key; only the paranoid guard checks it
- **Category:** engine · **Resolution type:** Rust fix (by construction)
- **Sources:** `src/gcg_sim/engine/view.py:223-239`, `tests/paranoid.py:1-8`, `tests/conftest.py:1-2`, `docs/python/AI.md:500-502`, `docs/ARCHITECTURE.md:50-51`, `.claude/skills/gcg-refresh-data/references/engine-extension.md:30`, `aa0dd30`, `d9d41af`
- **Quotes:** “No view input depends on the”; “Engine cache guard: random-agent and greedy games check every cached derived view against a fresh computation; GCG_SIM_PARANOID_VIEW=1 runs the same check in every test”; “once rules management finds no damage reaching printed HP, it skips the state until the next change (st.touch() bumps the version; every write to decks, Units, Bases, damage, pairing or identities touches)”; “Derived view cache keys on turn player, turn, battles and history so no caller can read a stale view”
- **What is weak:** the cache key is (active player, turn, number of battles, last battle, history length) plus version bumps from `st.touch()`, and assumes no view input depends on phase or step; a missed `touch()` means a stale view or a missed destruction. An earlier stale-view bug was fixed in aa0dd30; several output-identical shortcuts are protected only by the opt-in `GCG_SIM_PARANOID_VIEW=1` guard.
- **Resolution plan:** in Rust, state fields are private and every mutation goes through methods that bump the version, so a missing touch cannot compile; the cache key is the version. A `paranoid` cargo feature recomputes every cached view and re-runs every skipped rules check, and runs in CI over the full test suite and the fuzz games.
- **Verification** (unit + fuzz): `cargo test --features paranoid` and 10,000 paranoid fuzz games with zero mismatches; a compile-fail test shows a direct field write outside the state module does not compile.
- **Status:** resolved by construction (Phase 3): `GameState` fields are `pub(crate)`, and other crates read them through accessors. The cache key is `ViewKey`, and `touch` is the only invalidation. The `paranoid` feature recomputes every cached view and re-runs every skipped check.
- **Evidence:**
  - The compile-fail doctest in `crates/gcg-engine/src/contracts.rs` (T-023).
  - `cargo test --release -p gcg-engine --features paranoid,testkit`: 244 passed in the P6-J gate.
  - `gcg-diff corpus --tier 1 --require-all --paranoid`: 3804/3804 (T-054).
  - Paranoid fuzz of the 10,000 Python fuzz games: every cached view recomputed and compared, no mismatch.
- **Ablation:** no AI effect (row J). It is built in, and the `search` and `baselines` sets are at 100% against Python's traces.

### W-50 — The listened-events fast path depends on the decklists; the listen cache is process-global
- **Category:** engine · **Resolution type:** Rust fix (by construction)
- **Sources:** `src/gcg_sim/engine/core.py:119-121`, `src/gcg_sim/engine/core.py:287-313`, `src/gcg_sim/engine/core.py:334`, `src/gcg_sim/testkit.py:144-145`, `src/gcg_sim/testkit.py:212-217`, `docs/ARCHITECTURE.md:50-51`, `.claude/skills/gcg-refresh-data/references/engine-extension.md:30`, `c6716f5`, `d9d41af`
- **Quotes:** “Unless it is a token, it must be in its owner's decklists”; “if len(_LISTEN_CACHE) > 256:”; “Fix test scenarios skipping the triggers of cards added after start: Scenario.add then puts the card into its owner's decklists, which the engine's listened events follow”; “Events that fire for every card (like `TURN_END`) go in its `always` set, or the fast path will skip them.”
- **What is weak:** `emit()` skips events no card in either decklist (or a token or Unit variant) can trigger on; a card created outside the decklists silently misses its triggers (test helpers `Scenario.add_raw`, `pair_raw`, `raw_hand` did). The listen cache is a process-global dict keyed by decklists and cleared wholesale above 256 entries; tests must clear it after script overrides. The `always` set is hand-maintained.
- **Resolution plan:** Rust computes the listened-event set per game at `new_game` and stores it in the game; every card-creation path (deck, token, variant, test helper) goes through one constructor that extends the set; the `always` set is derived from the IR (triggers with no subject filter). No global cache.
- **Verification** (unit + fuzz): a testkit card added after start triggers; the derived `always` set equals the hand-maintained Python set (or the difference is explained); the paranoid feature checks every skipped event has no trigger candidate over the fuzz games.
- **Status:** resolved, with the narrowing recorded in PLAN §12.2 P6-J. The plan text proposed an IR-derived `always` set and per-game sets as engine values. Instead, the engine keeps Python's formula exactly (`GameConst.listened`, one set per game, no global cache), because the listened set decides the view-cache states that the traces record. The per-game refinements are checks. Testkit cards added after the start join the decklists, and the set is recomputed.
- **Evidence:**
  - `crates/gcg-effects/tests/p6j_ir_tripwires.rs::w50_the_derived_always_set_is_within_pythons`: the IR-derived set is a subset of Python's `always_events`, and today equal to it: `{destroys_by_battle, turn_end}`.
  - The negative control `w50_a_new_delayed_trigger_event_is_detected`.
  - `crates/gcg-engine/tests/p6j_engine.rs::w50_a_card_added_after_start_triggers`.
  - The `paranoid` unlistened-event check over the paranoid corpus and fuzz.
- **Ablation:** no AI effect (row J).

### W-51 — The rules-management HP shortcut depends on a registry-global "no HP-lowering effect" flag
- **Category:** engine · **Resolution type:** Rust fix (identical output)
- **Sources:** `src/gcg_sim/effects/registry.py:356-376`, `src/gcg_sim/engine/core.py:857-875`, `src/gcg_sim/engine/core.py:917-921`, `d9d41af`, `e82f08c`
- **Quotes:** “Without HP-lowering effects a card's HP”; “rules management skips the derived view when no Unit or Base has damage reaching its printed HP plus its Pilot's (without HP-lowering effects HP is never lower, so nothing can be destroyed)”
- **What is weak:** `hp_reduction_possible` is computed over the whole card database, so one HP-lowering card anywhere disables the shortcut in every game; test-only text that lowers HP once broke it (fixed in d9d41af by recomputing on overrides); correctness is checked only by the paranoid guard.
- **Resolution plan:** Rust computes the flag per game from the cards that can appear in it (decklists, tokens, variants) and recomputes it on test overrides; since the shortcut is output-identical by construction, results do not change.
- **Verification** (unit + fuzz): regression test of the d9d41af fix (test-only HP-lowering override leaves no lethal-damaged Unit in play); paranoid fuzz with mixed decks reports zero skipped destructions.
- **Status:** resolved, with the narrowing recorded in PLAN §12.2 P6-J. The engine keeps the registry's flag verbatim (`GameConst.hp_reduction_possible`), because it decides `_rules_checked`, which the traces record. The per-game flag (`core.rs` `paranoid::game_lowers_hp`) drives a `paranoid` check that every skipped printed-HP scan destroys nothing.
- **Evidence:**
  - `core::paranoid::tests::per_card_flags_agree_with_the_registry` and `statmod_reprs_follow_the_registry_formula`.
  - `crates/gcg-engine/tests/p6j_engine.rs::w51_an_hp_lowering_override_leaves_no_lethal_damaged_unit`, the d9d41af regression.
  - `w51_the_per_game_flag_drives_the_printed_hp_check` (`paranoid`).
  - The paranoid corpus and fuzz.
- **Ablation:** no AI effect (row J).

### W-52 — GameState.clone shares pending, the derived view and history entries (CB-1)
- **Category:** engine · **Resolution type:** Rust fix (by construction)
- **Sources:** `src/gcg_sim/engine/state.py:546`, `src/gcg_sim/engine/state.py:558`, `src/gcg_sim/engine/state.py:570`, `src/gcg_sim/engine/observe.py:86-95`, `docs/ASSUMPTIONS.md:88`, `docs/PROGRESS.md:133-135`, `d9d41af`
- **Quotes:** “s._derived = self._derived”; “Fix determinize and permute_hidden writing sampled identities into the true game's history: clone() shares HistoryEvent objects”
- **What is weak:** the shallow clone shares the Decision object, the cached derived view and the `HistoryEvent` objects; `determinize` and `permute_hidden` once wrote sampled identities into the true game's history (CB-1): play was unaffected, but final digests depended on the searches (2 of 63 games changed).
- **Resolution plan:** Rust clones have value semantics (shared parts are immutable, e.g. persistent or `Rc` vectors with copy-on-write), and determinization takes `&GameState` and returns a new state, so it cannot write into its source.
- **Verification** (unit + e2e): Rust port of `tests/ai/test_search_side_effects.py` (true-state digest unchanged by `determinize`, `permute_hidden` and a whole decision of each search); the 63-game benchmark's final digests equal agent-free replays.
- **Status:** resolved by construction (Phase 3): `GameState` owns its cards, zones, battles, lasting effects, triggers, frames, `once_used`, history and the pending decision, and `clone` copies them. The shared parts are immutable: `Arc<GameConst>`, and the cached `Arc<Derived>`, whose one `OnceLock` is filled once per key.
- **Evidence:**
  - `crates/gcg-search/tests/ai_search_side_effects.rs`, the port of `tests/ai/test_search_side_effects.py`.
  - `crates/gcg-engine/tests/engine_properties.rs::clone_from_state_equals_clone_on_game_states` (T-058).
  - Every Python replay verifies under Rust byte for byte (T-004).
- **Ablation:** no AI effect (row J).

### W-53 — Registry indices are process-local, so saved states do not reload faithfully in another process
- **Category:** engine · **Resolution type:** Rust fix (content-addressed ids)
- **Sources:** `docs/python/AI.md:510-511`, `docs/REPORTS.md:115-117`, `src/gcg_sim/runner/digest.py:1-7`, `src/gcg_sim/runner/digest.py:26-27`, `src/gcg_sim/effects/registry.py:123-138`, `src/gcg_sim/engine/interp.py:1276-1283`, `src/gcg_sim/engine/interp.py:1305`, `src/gcg_sim/engine/interp.py:1592`, `src/gcg_sim/engine/core.py:392`, `src/gcg_sim/engine/state.py:202`
- **Quotes:** “a stable, process-independent `Lasting.effect_key` so saved states reload faithfully in another process.”; “stores some registry indices that are assigned in first-use order”
- **What is weak:** `GameState.to_json` stores registry indices interned in first-use order (continuous effects, filter sets, lazily registered system and delayed programs, once-per-turn keys); frozen StatMods create a new continuous entry per distinct amount, so the registry grows. Only the digest canonicalizes them, with `repr()`-based SHA-256 content ids, which ties digests to Python `repr`. Deferred proposal.
- **Resolution plan:** Rust uses content-addressed ids (SHA-256 of a canonical serialization of the IR node) for continuous effects, filters, programs and once-per-turn keys in its saved-state format; the equivalence digest keeps Python's canonical form (computed from the same content ids through the ported `repr`, verified against exported content ids).
- **Verification** (unit + e2e): save a mid-game state, reload it in a fresh process whose registry was built in a different order, continue: final digest equals uninterrupted play; digest vectors equal Python's on the equivalence seeds.
- **Status:** resolved: saved states name programs, IR continuous effects and filter sets by content id, and spell value effects inline. `canon::from_canonical` resolves them against whatever registry it is given. Rust registry ids are fixed by the IR file.
- **Evidence:**
  - `crates/gcg-engine/tests/p6j_engine.rs::w53_a_saved_state_reloads_in_a_fresh_process`: a child process builds a fresh registry, reloads the state and plays on to the uninterrupted game's final digest.
  - `w53_reload_in_this_process`.
  - The corpus snapshots that Python saved and Rust reloads: tier 1 and tier 2 at 100% (T-045).
- **Ablation:** no AI effect (row J).

### W-54 — Dead enum values: the PAYMENT decision kind and Step.ATTACK_TRIGGERS
- **Category:** engine · **Resolution type:** Rust fix (no behaviour change)
- **Sources:** `src/gcg_sim/engine/types.py:79`, `src/gcg_sim/engine/types.py:114`, `src/gcg_sim/engine/game.py:31-32`, `src/gcg_sim/engine/game.py:372-382`, `tests/perf/__pycache__/test_engine_dispatch.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “PAYMENT = "payment"”; “``Step.ATTACK_TRIGGERS`` is never entered”
- **What is weak:** EX-vs-normal payment is folded into the play action's `c` parameter (`ALT_PLAY_BASE`), so no code creates a PAYMENT decision and the policy has no scorer for it; `Step.ATTACK_TRIGGERS` is never entered (attack triggers resolve as a trigger batch before the attack-end check, rule 8-2-2).
- **Resolution plan:** Rust keeps both variants only for serialization compatibility (their string and integer values), marks them unreachable in the engine with a debug assertion, and documents the `c` encoding.
- **Verification** (fuzz): 10,000 fuzz games and the equivalence runs never produce a PAYMENT decision or the ATTACK_TRIGGERS step (assertion counts zero).
- **Status:** resolved: both variants stay for serialisation. `diag::assert_live_decision` and `assert_live_step` panic in debug and `paranoid` builds if the engine ever produces them. The `c` encoding of the play action is documented in `crates/gcg-engine/src/diag.rs`.
- **Evidence:**
  - Over the 10,000 Python fuzz games (856,034 decisions): 0 PAYMENT decisions and 0 ATTACK_TRIGGERS steps (`crates/gcg-diff/tests/p6j_runs.rs`).
  - The tier-1 corpus has no `payment` decision and no snapshot at step 50.
  - The paranoid corpus run executes the assertions.
- **Ablation:** no AI effect (row J). The assertions are compiled out of release builds.

### W-55 — Interpretations and N/A rules that hold only for the current card pool
- **Category:** engine · **Resolution type:** port-preserve + data tripwire
- **Sources:** `docs/ASSUMPTIONS.md:54`, `docs/CONFLICTS.md:1271-1284`, `docs/CONFLICTS.md:1091-1099`, `docs/research/RULES_INDEX.md:207`, `src/gcg_sim/engine/battle.py:150-151`, `src/gcg_sim/data/rules/rules_na.json:164-166`, `src/gcg_sim/data/rules/rules_na.json:284-291`, `e911333`, `97d3e08`, `7a797fb`, `19886fe`
- **Quotes:** “Bases deal no battle damage to the attacking Unit (every Base has 0 AP).”; “Re-test if a card that places several Resources from the resource deck is ever added.”; “Nothing in the pinned card pool moves more than one card out of the resource deck at a time, so this rule never changes what the engine does.”; “Regenerate RULES_TRACEABILITY from the full JUnit run: 395 tested, 184 N/A, 0 missing”; “Accept N/A reasons for token-supply and resource-deck multi-move rules; update index tests”
- **What is weak:** several rules decisions are safe only because no current card breaks them: Bases have 0 AP and deal no battle damage back (contradicts the literal 3-5-4 and 13-1-5-2; First Strike against a Base changes nothing); rule 4-3-3 (simultaneous multi-card moves from the resource deck) is unimplemented and N/A; there is no token supply or token-count limit (5-17-4-1, 5-17-4-3). 184 rules are N/A by design, some of them pool-dependent. A future card could silently make any of these wrong.
- **Resolution plan:** the Rust engine implements the decisions exactly. A data-gate test suite ("tripwires") fails when a packaged card breaks a precondition: a Base with non-zero AP, any IR step that moves more than one card from the resource deck at once, any IR that needs a token supply, and, for every N/A reason in `rules_na.json` classified as pool-dependent, a check of its precondition. A failing tripwire blocks a data refresh until the rule is implemented.
- **Verification** (unit + negative control): tripwires pass on the packaged pool; a synthetic card in a temporary data copy (Base with AP 1; a "place 2 Resources" step) makes the matching tripwire fail.
- **Status:** resolved: tripwires guard every pool-dependent decision: Base AP 0, rule 4-3-3, and the token supply. Each pool-dependent N/A reason of `rules_na.json` has its tripwire.
- **Evidence:**
  - `crates/gcg-data/tests/p6k_data_tripwires.rs::w55_bases_have_zero_ap_with_negative_control`.
  - `crates/gcg-effects/tests/p6k_ir_scans.rs::w55_resource_deck_moves_and_token_supply_with_negative_control`.
  - `w55_every_pool_dependent_na_reason_has_a_tripwire`: 4-3-3, 5-17-4-1, 5-17-4-3, 5-17-4-4 and 6-1-2-1.
- **Ablation:** no AI effect (row K).

### W-56 — Rules-interpretation decisions the port must keep, pinned by rulings and curated conflicts
- **Category:** engine · **Resolution type:** port-preserve + conformance tests
- **Sources:** `docs/ASSUMPTIONS.md:56`, `docs/ASSUMPTIONS.md:59-61`, `docs/ASSUMPTIONS.md:63`, `docs/ASSUMPTIONS.md:65-71`, `docs/ARCHITECTURE.md:88-89`, `docs/CONFLICTS.md:1286-1300`, `docs/CONFLICTS.md:1302-1312`, `docs/CONFLICTS.md:1314-1327`, `docs/CONFLICTS.md:1617`, `docs/CONFLICTS.md:1973`, `docs/CONFLICTS.md:2384`, `docs/CONFLICTS.md:2798`, `docs/CONFLICTS.md:3128`, `docs/CONFLICTS.md:4072`, `docs/CONFLICTS.md:4098-4105`, `docs/CONFLICTS.md:4216`, `docs/CONFLICTS.md:4729-4737`, `docs/CONFLICTS.md:4784-4792`, `docs/CONFLICTS.md:5389-5398`, `docs/CONFLICTS.md:5411-5423`, `docs/CONFLICTS.md:5425-5436`, `docs/CONFLICTS.md:5452-5463`, `docs/CONFLICTS.md:5465-5477`, `docs/CONFLICTS.md:5479-5490`, `docs/research/RULES_INDEX.md:201-211`, `src/gcg_sim/engine/game.py:116-117`, `src/gcg_sim/engine/interp.py:1033-1038`, `src/gcg_sim/engine/interp.py:1051-1055`, `src/gcg_sim/engine/core.py:18-19`
- **Quotes:** “Non-token cards go to the removal area (5-12); tokens leave the game (5-17-2-5).”; “A triggered effect is used up only if it performs an action (paying a cost counts).”; “A Unit with 0 AP that attacks a player with no Base and no Shields deals no damage, so the player is not defeated.”; “return False # rules 4-4-2, 4-4-2-1”; “Follow 10-2-2-1 literally: removal-area cards are not "targets"”; “Sets 1 rested Resource active, normal Resources before EX Resources, without a player choice.”
- **What is weak:** a family of recorded interpretation choices, each open to a future official ruling and each easy to break silently in a re-implementation: "exile" = removal area (tokens leave the game; an EX Resource paid as a cost counts as exiled for GD05-018); placement beyond 15 Resources / 5 EX does nothing; lasting "all your Units" effects apply only to Units present at resolution (Q105); uids assigned after the shuffle and cut; owner-ordered simultaneous placement into private zones; 【Once per Turn】 used up only when the effect acts (Q197); delayed "During this turn, when …" triggers repeat; "during this battle" effects outside a battle do nothing; destruction cause tracking (effect damage vs "destroy", Q287, Q368); modal Commands need a fully targetable mode; attack-target constraint priority (GD04-107, Q289–Q301, Q388); "deploy it as an (APx･HPy) Unit" (Q389, Q390); removal-area cards are not targets; last placed Shield is on top; a 0-AP attack on an empty-shield player does not defeat; 【When Linked】 triggers only on pairing; <Suppression> with a Base present does nothing; old <Breach> reminder wording ignored; "pay its cost to deploy it" skips the Lv. check; "If …"/"If you do" govern a following "Then" (Q175, Q233); resolutions that remove a player decision (ST14-015, GD03-129, GD03-076).
- **Resolution plan:** Rust implements each exactly as in Python (rules unchanged). Every decision gets its ASSUMPTIONS/CONFLICTS anchor as an interpretation id attached to the Rust ports of the tagged Python tests (`rule`, `ruling`, `faq`, `card` tags), and a generated traceability table maps id → Rust tests. The data-refresh gate lists every new official FAQ or ruling that cites a rule or card of one of these ids for re-review.
- **Verification** (unit + differential): all ported tagged tests pass; differential fuzz (Python vs Rust final digests) over 10,000 random games with generated decks containing the affected cards (EB01-070, GD01-058, GD01-082, GD02-105, GD02-106, GD02-119, GD04-107, GD05-089, ST14-015 and the 17 <Suppression> and 41 <Breach> cards) shows zero differences.
- **Status:** resolved: 27 interpretation ids, I-01 to I-27 (`docs/port/audit/interpretations.{json,md}`). Each has its anchor, tags, Python tests and the per-test traces that replay them through the Rust engine. New rulings on an interpretation's cards fail a test, so a data refresh lists them for re-review. Deviation (stated in `docs/port/eval/weakness-status/P6-K.md`): no fresh 10,000-game run of per-game final digests was made. The corpus's per-decision digests are used instead: a stronger check per game, on fewer games.
- **Evidence:**
  - `crates/gcg-engine/tests/p6k_audit.rs::w56_every_interpretation_id_has_passing_rust_traces`: 2 to 149 traces per id.
  - `w56_new_rulings_on_interpretation_cards_are_listed_for_re_review`.
  - `uv run python port/oracle/p6k_audit.py --check`.
  - The corpus: 518 coverage games and 600 fuzz games, with a digest at every decision (tier 1 3804/3804, tier 2 4154/4154).
  - T-055: 10,000 fuzz games, summary byte-equal to Python.
- **Ablation:** no AI effect (row K).

### W-57 — Historical engine, validator, runner and tooling bugs fixed in Python: regression risk for the port
- **Category:** engine · **Resolution type:** port-preserve + regression tests
- **Sources:** `102e72b`, `7c47b86`, `ddb9f4f`, `0e1d3e8`, `0ee591d`, `e911333`, `013ea8f`, `816f9e4`, `c8cc8e9`, `f89c0cc`, `4dd6701`, `39acfd1`, `fb51ba3`, `6283471`, `97d3e08`, `ca93dff`, `7d290d6`, `48d152f`, `f78574e`, `62af367`, `f042788`, `3e37b60`, `e16e494`, `118f324`, `db94768`, `7e52f58`, `331b70f`, `9408681`, `38aaf43`, `21489cb`, `ecdacf9`, `e24ed91`, `15ae2eb`, `ef05730`, `d9d41af`
- **Quotes:** “Fix engine/compiler bugs from rule-tagged tests: Then-scope, choose-N and grouped targets, instead substitutions, Development, play modifiers, name aliases, after-Main pairing”; “engine deck check counts printings together (2-1-1)”; “effect-begun battles see battling effects (Q447, Q455)”; “clean errors for non-UTF-8 replay files and failed report writes naming the file”; “--rounds 0 no longer mislabels the start weights”
- **What is weak:** 89 historical strict xfails were added in 26 commits and closed in 102e72b (26), 7c47b86 (22), ddb9f4f (3), 0e1d3e8 (16) and 0ee591d (22); none remain. Fixed bugs include: battle LKI and Base damage; <Breach> amount read from the destroyed host; 【Destroyed】 ignoring the last-known paired Pilot; HP filters using printed HP (Q96); <Support> bonus lost with its source (Q106); no concession entry point; "All players draw 1" defeating only the active player; `new_game` accepting non-conforming decks; phase/step labels advancing early; deck placement ordering and information leaks (4-1-6, 4-1-7, 4-2-2); "during this battle" effects never expiring outside a battle (re-found in at least 11 commits); a Unit trashed by an effect not treated as destroyed; play modifiers skipped; missing COST_PAID; Once per Turn keys and LKI zone_seq; missing command-activation events; effect-damage destruction attribution; one-shot delayed triggers; modal Commands with no legal mode; shield damage prevention and thresholds; paired Pilot destruction; forced/attracted attack targets; reused rest substitutes and shared lasting keys; HasKeyword off the field; redirected battle damage; turn-history gaps; effect-activated paired Commands; effect-begun battles; simultaneous "all players each" effects; multi-token deployments; printings counted separately in the deck check; FAQ Q1–Q6 once N/A; unclean errors for non-UTF-8 files; `tuning --rounds 0` mislabelling.
- **Resolution plan:** port every tagged Python test (rule, card, FAQ and ruling tags) with its tag; a regression table in the Rust test tree maps each historical xfail id and each fixed bug above to the Rust test that covers it; Python-vs-Rust differential replays run in CI.
- **Verification** (unit + meta-test): all ported tests pass; a meta-test fails if any historical xfail id or listed bug lacks a Rust test; differential fuzz shows zero digest differences.
- **Status:** resolved: `docs/port/audit/regressions.{json,md}` maps the 90 historical strict-xfail tests and the 34 fixed bugs to their Python tests, per-test traces and named native Rust tests. One bug row, `tuning --rounds 0`, has no Rust counterpart by design. The P6-K file says P6-I's `gcg-eval tune` must carry that test, and no such test is recorded.
- **Evidence:** `crates/gcg-engine/tests/p6k_audit.rs::w57_w70_every_historical_fix_has_a_rust_test`. It fails if any xfail or bug lacks passing Rust evidence.
- **Ablation:** no AI effect (row K).

## Effects

### W-58 — Most-defensible readings of ambiguous card text and major ruling overrides
- **Category:** effects · **Resolution type:** port-preserve + reporting
- **Sources:** `README.md:170-171`, `docs/CONFLICTS.md:75-90`, `docs/CONFLICTS.md:1259-1314`, `docs/CONFLICTS.md:4282-4284`, `docs/CONFLICTS.md:5334-5343`, `docs/PROGRESS.md:32-34`, `data/conflicts.json`, `da570b3`
- **Quotes:** “Where no official ruling exists, the most defensible reading was chosen and recorded there.”; “Total: 402 conflicts; unresolved: 0”; “Curate 103 card-agent readings of ambiguous text as CONFLICTS entries with verified quotes and resolutions”; “No official source settles the question”
- **What is weak:** 402 conflicts: 108 ambiguous card text (106 minor), 192 rulings vs card text (4 major), 16 FAQ vs rules; by policy, 110 `most_defensible_reading`, 192 `ruling_overrides_text`, 45 `canonical_printing`, 16 `faq_clarifies_rules`, 11 `rules_xref_intended_target`, 6 `banlist_most_defensible`. Major: errata ST12-001, markers GD02-053, rulings GD02-021:Q175, GD03-072:Q233, GD04-101:Q287, GD05-054:Q368. Behaviour on these cards (e.g. ST13-013 simultaneous 1-to-2 token deploys) may change if Bandai rules differently.
- **Resolution plan:** Rust reads `overrides.json` and the conflicts data as packaged (no re-encoding, no change); reports list every interpretation that affected a run (W-101); the refresh gate's re-review list (W-56) also covers these conflict ids.
- **Verification** (unit + e2e): data-loading test: 402 conflicts, 0 unresolved, policy counts equal `data/conflicts.json`; e2e: a run with a deck containing ST13-013 lists that conflict id.
- **Status:** resolved: a data-loading test of the conflicts report, and an end-to-end check that a deck's conflict ids are listed.
- **Evidence:**
  - `crates/gcg-data/tests/p6k_data_tripwires.rs::w58_conflict_counts_and_policies`: 402 conflicts, none unresolved, invalid, stale or orphaned. Policy counts: 110 `most_defensible_reading`, 192 `ruling_overrides_text`, and the others.
  - `w58_a_deck_with_st13_013_lists_its_conflict_ids`.
  - The major conflicts are I-10, I-21, I-25, I-26 and I-27 on the W-56 re-review list.
- **Ablation:** no AI effect (row K).

### W-59 — Pilot text routing is inferred, not encoded in the data
- **Category:** effects · **Resolution type:** port-preserve + audit table
- **Sources:** `docs/ASSUMPTIONS.md:58`, `docs/research/DATA_INGESTION.md:280-284`, `docs/research/EFFECT_TEMPLATES.md:762-768`, `src/gcg_sim/effects/compiler/__init__.py:32-49`
- **Quotes:** “the data does not mark which lines are above the name.”; “This split is inferred from the text, not encoded in the data.”; “【Burst】 and location-specific effects stay with the Pilot card.”
- **What is weak:** gcg-api joins both Pilot texts into one effect string; 【Burst】, name-alias, play-modifier and hand/trash lines stay with the Pilot and everything else goes to the paired Unit. A misattributed line would be a silent rules bug.
- **Resolution plan:** the Rust effect layer reproduces the same split (exported IR or ported compiler); a generated audit table (card → lines kept by the Pilot vs granted to the Unit) is checked in with the Rust test data, so any change in the split, including for new Pilot cards, shows up as a reviewed diff.
- **Verification** (unit): the audit table regenerates identically from the packaged data; Rust and Python splits agree for every Pilot card.
- **Status:** resolved: `docs/port/audit/pilot-routing.md` gives, for each of the 127 Pilot cards, the abilities the Pilot keeps and those the paired Unit gains. It is generated from the Python registry and regenerated from the Rust IR by the test.
- **Evidence:** `crates/gcg-effects/tests/p6k_ir_scans.rs::w59_pilot_routing_table_regenerates_from_the_ir` and `uv run python port/oracle/p6k_audit.py --check`.
- **Ablation:** no AI effect (row K).

### W-60 — Explanatory parentheticals are stripped but carry semantics the bindings must honour
- **Category:** effects · **Resolution type:** port-preserve + tests
- **Sources:** `docs/research/EFFECT_TEMPLATES.md:91-95`, `docs/research/DATA_INGESTION.md:137-141`, `.claude/skills/gcg-refresh-data/references/card-checklist.md:21-22`
- **Quotes:** “The bindings for those cards must still honour them.”; “they state intended semantics, so effect implementers should read the raw text”
- **What is weak:** reminder-text normalization (rule 2-11-4) removes "(Don't treat it as a Pilot.)" (GD05-089), "(You choose first, then your opponent.)" (ST12-013) and "(This card that is being activated is not counted.)" (ST14-014), yet the bindings must implement them.
- **Resolution plan:** a Rust test per affected card checks the parenthetical's behaviour (GD05-089 is not a Pilot while deployed as a Unit; ST12-013 choice order; ST14-014 does not count itself), and the data gate lists every stripped parenthetical that is not plain reminder text so new ones get a test. W-155 extends that gate to the parentheticals' text, so a reworded one is caught too.
- **Verification** (unit): the three behaviour tests pass in Rust and Python; the stripped-parenthetical list on the packaged data contains exactly the known cards.
- **Status:** resolved: a native behaviour test per card, in Rust and in Python, plus the stripped-parenthetical list (W-155's gate).
- **Evidence:**
  - Rust: `crates/gcg-engine/tests/p6k_behaviour.rs::w60_gd05_089_deployed_as_a_unit_is_not_a_pilot`, `w60_st12_013_you_choose_first_then_your_opponent` and `w60_st14_014_does_not_count_itself`.
  - Python: `port/oracle/tests/test_p6k_parentheticals.py`.
  - `crates/gcg-data/tests/p6k_data_tripwires.rs::w155_parenthetical_gate_passes_on_the_packaged_data`.
- **Ablation:** no AI effect (row K).

### W-61 — Grammar slips and duplicated token clauses in card text break templates
- **Category:** effects · **Resolution type:** port-preserve + tests
- **Sources:** `docs/research/EFFECT_TEMPLATES.md:934-939`
- **Quotes:** “**Grammar slips that break templates:** ST09-004 ("While a friendly Base in play"),”
- **What is weak:** ST09-004 ("While a friendly Base in play"), GD02-086 ("this gets AP+1"), GD05-101 ("one of your Unit's effects"), GD04-100 ("pay ① or more cost") and tokens T-014/T-021/T-022/T-029 (duplicated definition clauses) need bindings or tolerant templates.
- **Resolution plan:** Rust keeps the same per-card behaviour; each listed card has a behaviour test, and the compiler's tolerance list (if the compiler is ported) is data-driven and tested against these exact texts.
- **Verification** (unit): behaviour tests for the eight cards pass in Rust with the same golden IR as Python.
- **Status:** resolved: the eight cards keep their per-card behaviour. No compiler is ported, so no tolerance list is needed.
- **Evidence:**
  - `crates/gcg-engine/tests/p6k_audit.rs::pinned_cards_have_passing_rust_behaviour_tests`: ST09-004, GD02-086, GD05-101, GD04-100, T-014, T-021, T-022 and T-029.
  - The golden IR equals Python's (`all_golden_files_regenerate_byte_for_byte`, T-042).
- **Ablation:** no AI effect (row K).

### W-62 — 'instead' wording: conditional overrides vs true substitutions
- **Category:** effects · **Resolution type:** port-preserve + tests
- **Sources:** `docs/research/EFFECT_TEMPLATES.md:795-806`
- **Quotes:** “Compile these as a conditional override of the previous step's parameters, not as a rule-10-1-9 substitution.”
- **What is weak:** twelve "instead" texts are conditional parameter overrides of the previous step; the true 10-1-9 substitutions are GD03-079 and GD05-124 (rest substitution), GD04-087 and GD04-095 (damage redirect), ST12-012 and GD05-089. Confusing the two changes card behaviour.
- **Resolution plan:** Rust models the two as different IR nodes (override vs replacement effect) with no shared code path; each of the 18 cards has a behaviour test.
- **Verification** (unit): 18 behaviour tests pass in Rust; a test asserts every "instead" text in the pool maps to exactly one of the two node kinds.
- **Status:** resolved: a reviewed table of the 18 "instead" cards. 12 are conditional overrides and 6 are rule 10-1-9 substitutions.
- **Evidence:**
  - `crates/gcg-effects/tests/p6k_ir_scans.rs::w62_every_instead_text_maps_to_exactly_one_kind`.
  - `pinned_cards_have_passing_rust_behaviour_tests`: all 18 have passing traces.
- **Ablation:** no AI effect (row K).

### W-63 — The regex compiler covers about 64% of non-vanilla cards; 302 per-card bindings; new sets need manual multi-agent work
- **Category:** effects · **Resolution type:** general templates, golden-identical (Rust or Python side, after the IR decision)
- **Sources:** `docs/status.json:18-24`, `docs/research/EFFECT_TEMPLATES.md:34-42`, `docs/research/EFFECT_TEMPLATES.md:911-915`, `docs/ARCHITECTURE.md:28-29`, `docs/ARCHITECTURE.md:96-99`, `src/gcg_sim/effects/compiler/__init__.py:1-5`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:144`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:173`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:186`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:265`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:320`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:384`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:428`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:437`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:197`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:246`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:559`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:577`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:618`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:707`, `.claude/skills/gcg-refresh-data/SKILL.md:318-331`, `CLAUDE.md:44-47`, `docs/port/inventory/04-effects-ir.md:950-952`
- **Quotes:** “The last 189 cards would need 225 single-use templates, one or two per card, so per-card bindings are the cheaper route for them.”; “Any fragment it cannot recognise raises :class:`CompileError`;”; “recommend to the user that they run it with `ultracode`”; “Will the Rust engine load a Python-exported IR (recommended) or must it also compile card text”
- **What is weak:** status: 538 compiled, 302 binding, 308 vanilla; the research estimated 189 binding cards but 302 exist. About 38 binding notes say "compile error" (selector suffixes such as "paired with this Unit", delayed "during this turn, when …", inline token definitions, "when you play and activate … using an EX Resource", "are deployed rested", conditional damage reduction, pronoun "it" without antecedent, "exile this card in your trash", "It can't activate <Blocker>"). Large releases need about 65 card numbers per package of manual binding work. The project rule prefers general templates over per-card hooks.
- **Resolution plan:** first, the lead records the open port decision: does Rust load a Python-exported IR (the inventory's recommendation) or port the text compiler (`04-effects-ir.md:950-952`)? The plan then depends on the answer. (a) Exported IR: the general templates for the recurring compile-error families above are added to the Python compiler through the binding workflow (handed to the lead), and the Rust work is only the IR node types they need. (b) Ported compiler: the templates or DSL nodes are added in the Rust effect layer. Either way, the bindings they make redundant are retired, and a change is accepted only when the card's golden IR behaviour is identical (golden check empty, differential fuzz clean). Bindings stay where no general form exists, and card behaviour does not change.
- **Verification** (unit + differential): per retired binding, the golden record and the card's tagged tests are identical in Rust; differential fuzz over decks containing the affected cards shows zero digest differences; the coverage report shows the binding count falling.
- **Status:** resolved: the in-scope part is done (§12.11, lead decision (a)). By the lead's decision (`docs/port/PLAN.md` §0.1, end of Phase 6), `port/oracle/templates/check.py --check` is T-154's assertion that every template reproduces its binding cards byte for byte, and the IR gets no `cards[].template` field: no Rust code path reads it, and it would change the IR bytes and so every corpus trace's `ir_sha256` (`docs/port/eval/weakness-status/GAP-C.md`). Done:
  - `port/oracle/templates/` holds 20 general templates, with a template for every listed compile-error family. With them 24 of the 302 binding cards compile byte for byte to their bindings' scripts (6 before), and no compiled card changes. Six binding cards stay listed as reviewed differences, each with its reason.
  - `gcg-eval data-status --templates` prints the per-template counts from the committed report.
- **Post-acceptance:** move the templates into `src/gcg_sim/effects/compiler/` and retire the reproduced bindings with an empty golden diff (§12.11 answer (a)). The plain compiler already reproduces 32 more binding cards with no template (report key `compiler_reproduces_bindings`), so their bindings can be retired the same way.
- **Evidence:**
  - `uv run python port/oracle/templates/check.py --check`, run in `scripts/verify_all_rust.sh --full`. It recompiles every compiled card (538) with the templates and requires its committed script, and checks every binding card (302).
  - `docs/port/audit/templates.json`: 20 templates, 24 bindings reproduced, 6 reviewed differences, 0 mismatches.
  - `port/oracle/tests/test_p6k_templates.py` (the pinned map, 24 per-card equalities, the table restore, two negative controls).
  - `crates/gcg-eval/tests/gapc_tools.rs::w63_data_status_templates_reports_the_committed_template_report`.
  - `target/release/gcg-eval data-status --templates` (run 2026-11-04): "templates: 20 general template(s) reproduce 24 of 302 binding cards byte for byte; 278 bindings remain".
  - Families: selector suffixes GD03-049; delayed "during this turn, when …" GD04-002; inline token definitions ST04-012; the EX-Resource trigger GD04-020; "it" without antecedent EB01-086, ST06-015, GD05-127, GD04-041; "It can't activate <Blocker>" GD05-127; conditional damage reduction GD05-094, GD05-125. "are deployed rested" (GD04-022) and "exile this card in your trash" (GD05-130) have templates whose cards stay reviewed differences.
- **Ablation:** no AI effect (row K).

### W-64 — Card-specific custom hooks, several duplicated; custom steps cannot request choices
- **Category:** effects · **Resolution type:** Rust fix (consolidated hooks, behaviour-identical; form depends on the IR decision)
- **Sources:** `src/gcg_sim/effects/dsl.py:6-7`, `src/gcg_sim/engine/interp.py:1-6`, `src/gcg_sim/effects/builtins.py:13-65`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:24`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:52-61`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:115-121`, `src/gcg_sim/effects/bindings/wp_gd05_a.py:97`, `src/gcg_sim/effects/bindings/wp_gd05_a.py:122`, `src/gcg_sim/effects/bindings/wp_gd01_b.py:117`, `src/gcg_sim/effects/bindings/wp_st12_14.py:83`, `src/gcg_sim/effects/bindings/wp_st12_14.py:96`, `src/gcg_sim/effects/bindings/wp_st05_08.py:46`, `src/gcg_sim/effects/bindings/wp_eb01_a.py:61`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:240`, `src/gcg_sim/effects/bindings/wp_gd02_b.py:54`, `src/gcg_sim/effects/bindings/wp_gd03_a.py:92`, `CLAUDE.md:44-47`, `docs/port/inventory/04-effects-ir.md:950-952`
- **Quotes:** “modules are deterministic and never request choices.”; “Prefer a general compiler template or a DSL/engine feature over a card-specific hook.”; “Will the Rust engine load a Python-exported IR (recommended) or must it also compile card text”
- **What is weak:** 29 hooks (10 `@custom_step`, 8 `@custom_filter`, 6 `@custom_cond`, 5 `@custom_value`) plus 5 builtins; duplicated purposes: `played_with_ex` (gd04_b, gd05_b, with different semantics: most recent play vs this turn), destroyed-by-effect / with-damage (gd05_a, gd01_b, st12_14), look at own top card (eb01_a, eb01_b), pay cost to deploy (gd02_b, gd03_a), lowest/highest enemy Lv. (gd05_a, st12_14, st05_08). Custom steps cannot request choices.
- **Resolution plan:** depends on the IR decision recorded in W-63. Custom hooks are Python functions, so in both cases Rust implements each hook natively, keyed by its hook id. (a) Exported IR: the exported IR keeps the Python hook ids. Rust maps each duplicated family to one implementation, with each card's exact semantics as a parameter where the copies differ, so no Python change is needed. Turning a family into a real DSL node is a Python binding change handed to the lead. (b) Ported compiler: each duplicated family becomes one typed DSL node in the Rust effect layer, keeping each card's exact semantics as a node parameter. In both cases, hooks without duplicates stay as named Rust functions in one module, and a change is accepted only when golden-identical (as W-63).
- **Verification** (unit + differential): every card using a consolidated hook passes its tagged tests and golden check; a test asserts no two hooks share a name-normalised purpose; differential fuzz clean.
- **Status:** resolved: a checked purpose table covers all 34 custom hooks (`crates/gcg-effects/tests/p6k_ir_scans.rs` `HOOK_PURPOSES`). No two hooks share a purpose except the declared identical alias (`wp_eb01_a_look_own_top`/`wp_eb01_b_look_top_own`, one engine body). Every duplicated family is now one engine implementation, with each card's exact semantics as a parameter (`crates/gcg-engine/src/view/hooks.rs`; `docs/port/eval/weakness-status/GAP-B.md`): `played_with_ex(st, ctx, ExScope)` for the GD04 and GD05 pair, and `destroyed(st, ctx, Cause)` for the three destroyed-cause conditions. The look-own-top pair and the enemy-level family were already one body each. "Custom steps cannot request choices" stays as in Python, a stated decision.
- **Evidence:** these pass:
  - `view::hooks::tests::w64_the_destroyed_cause_predicate_is_the_three_hooks` (64 combinations of battle, cause, controller and frames, against the pre-W-64 bodies kept verbatim as oracles) and `w64_the_played_with_ex_scan_is_the_two_hooks` (all 9,331 histories of up to 5 records over 2 uids and 3 kinds).
  - `crates/gcg-effects/tests/p6k_ir_scans.rs::w64_no_two_hooks_share_a_purpose`.
  - The per-test traces of the GD04, GD05, GD01 and ST12 cards in the tier-1 `tests` set (3213/3213), the tier-1 corpus at 3804/3804, and the differential fuzz `gcg-diff fuzz --games 10000 --seed 2026 --compare port/corpus/v1/tier1/fuzz/summary-2026-10000.json` (10,000 games, 856,034 actions, 0 failures, equal to Python's summary), all recorded in GAP-B. GAP-B did not run tier 2; the merged gap-closing gate records tier 2 at 4154/4154 (commit message of b8483dc).
- **Ablation:** no AI effect (A13): behaviour-identical by construction, with the `search` (9/9) and `baselines` (8/8) corpus sets and the seed-1 equivalence smoke (3/3) unchanged (GAP-B).

### W-65 — The keyword and timing vocabulary is duplicated across many files
- **Category:** effects · **Resolution type:** Rust fix (single source)
- **Sources:** `.claude/skills/gcg-refresh-data/references/engine-extension.md:9-31`
- **Quotes:** “The keyword vocabulary appears in several places. Update all of them.”
- **What is weak:** the vocabulary lives in `dsl.Kw`, the compiler's `abilities._KW`, `nouns.KW_RE`, `grammar._KWS`, the conflicts `KEYWORD_VOCABULARY`/`TIMING_VOCABULARY`, the registry's keyword programs and the AI evaluation.
- **Resolution plan:** Rust defines keywords and timings once (an enum with its text forms and program), and every other table is derived from it with exhaustive matches, so adding a keyword forces every consumer to handle it.
- **Verification** (unit): a test asserts the derived tables equal the Python vocabularies on the packaged data; adding a variant without handling fails to compile.
- **Status:** resolved: the Rust keyword and timing enums are the single source. The test derives each Python vocabulary from them with exhaustive matches.
- **Evidence:** `crates/gcg-effects/tests/p6k_ir_scans.rs::w65_python_vocabularies_derive_from_the_rust_enums`, against `port/oracle/tests/vectors/p6k_vocabulary.json`, and `uv run python port/oracle/p6k_vectors.py --check`.
- **Ablation:** no AI effect (row K).

### W-66 — The compiler miscompiled some cards silently; bindings fixed the cases found
- **Category:** effects · **Resolution type:** Rust fix (differential audit)
- **Sources:** `src/gcg_sim/effects/bindings/wp_gd04_a.py:229`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:489`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:508`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:539`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:553`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:669`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:684`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:334-336`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:395`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:450-452`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:169`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:232`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:290`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:316`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:210`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:223`
- **Quotes:** “compiled AP reduction recounted the trash continuously instead of fixing it”
- **What is weak:** binding notes record silent miscompiles: resting one Unit when the other target was missing (Q265); an optional choice without a minimum; damaging self with no enemy chosen; a Pilot referenced through the destroyed Unit; only the first mode compiled and the second run unconditionally; friendly attacks retargeted; an event payload read in a Command frame that lacks it; "belonging to each enemy player" compiled as either side; a dropped "… and 1 enemy Unit"; an EX-Resource variant granting nothing. Other compiled cards may share the same patterns.
- **Resolution plan:** a pattern audit: for each miscompile family, a static scan of the IR of every compiled (non-binding) card for the same shape (e.g. a two-target step without an all-targets guard, an optional choice without minimum, a mode list whose second mode is unconditional); each hit gets a tagged behaviour test against the card's text and rulings. Fixes go through the Python data/binding workflow if a real miscompile is found, since card behaviour must not change in the port alone.
- **Verification** (unit): the scan runs in the Rust test suite and lists zero unreviewed hits; each reviewed hit has a tagged test passing in both implementations.
- **Status:** resolved: no real miscompile was found. A static scan of the 538 compiled cards' IR covers eight families. It has 5 hits, all reviewed and not miscompiles: GD01-103, GD02-055, GD03-116, GD04-114 and GD05-117. The engine's multi-target pre-check makes them need both targets.
- **Evidence:**
  - `crates/gcg-effects/tests/p6k_ir_scans.rs::w66_miscompile_scan_lists_zero_unreviewed_hits`.
  - The negative control `w66_scan_finds_the_binding_fixed_miscompiles_in_their_compiled_form`.
  - `crates/gcg-engine/tests/p6k_behaviour.rs::w66_two_target_commands_need_both_targets` and `w66_gd02_055_deploy_needs_both_targets`.
  - Python: `port/oracle/tests/test_p6k_miscompile.py` (5 tests).
- **Ablation:** no AI effect (row K).

### W-67 — No templates for "all players each …" and related multi-player-step wording
- **Category:** effects · **Resolution type:** general templates, golden-identical (Rust or Python side, after the IR decision)
- **Sources:** `src/gcg_sim/effects/bindings/wp_eb01_b.py:89`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:103`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:135`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:161`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:221`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:277`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:144-147`, `docs/port/inventory/04-effects-ir.md:950-952`
- **Quotes:** “'All players each choose 1 of their Resources' has no template (Q321-Q323)”; “Will the Rust engine load a Python-exported IR (recommended) or must it also compile card text”
- **What is weak:** also missing: "place the top card of your deck into your trash", "They may draw 1", "reveal 1 … return it to the top", "all enemy players each choose", "All players each look at the top card", and "After activating this card's 【Main】, you may pair this card from your trash" (modelled inline).
- **Resolution plan:** depends on the IR decision recorded in W-63. The content of the change is one general "each player in turn order" step (with the simultaneous rules-management deferral of Q319/Q331) plus the listed single-player templates. (a) Exported IR: the step and the templates go into the Python DSL and compiler through the binding workflow (handed to the lead), and Rust adds the one IR node the step needs. (b) Ported compiler: both go into the Rust effect layer. Either way, the matching bindings are retired only when golden-identical (as W-63).
- **Verification** (unit + differential): golden and tagged tests for every affected card identical; differential fuzz clean.
- **Status:** resolved: as W-63, by the same lead decision (`docs/port/PLAN.md` §0.1: `check.py --check` is T-154's assertion; no IR `template` field). The in-scope part is done (§12.11): templates for the "each player in turn order" step and every listed single-player wording, through W-63's mechanism, and `gcg-eval data-status --templates`.
- **Post-acceptance:** W-67's single "each player in turn order" DSL node would need a Python DSL and interpreter change. In scope, the template emits the binding's node composition (§12.11).
- **Evidence:** the checker and report of W-63 (`port/oracle/templates/check.py --check`, `docs/port/audit/templates.json`, `gapc_tools.rs::w63_data_status_templates_reports_the_committed_template_report`). Bindings reproduced byte for byte:
  - "each player in turn order" / "All players each look at the top card" (active then standby, rulings Q319 and Q331): EB01-023 (EB01-078 a reviewed difference);
  - "All players each choose 1 of their Resources": EB01-059;
  - "They may draw 1": EB01-062;
  - "reveal 1 … return it to the top": EB01-067;
  - "all enemy players each choose": EB01-009 and GD05-049 (EB01-074 a reviewed difference);
  - "place the top card of your deck into your trash": GD02-127, GD03-092, GD03-094 and EB01-050 (GD03-129 a reviewed difference);
  - "After activating this card's 【Main】, you may pair …": GD05-112, GD05-113, GD05-121 and GD05-122.
- **Ablation:** no AI effect (row K).

### W-68 — Stale "compiled to printed HP" binding notes: Stat.HP already means current HP
- **Category:** effects · **Resolution type:** Rust fix (dedupe, behaviour-identical)
- **Sources:** `src/gcg_sim/engine/view.py:673-685`, `src/gcg_sim/effects/bindings/wp_gd01_a.py:78-85`, `src/gcg_sim/effects/bindings/wp_st01_04.py:19`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:34-36`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:164`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:186`
- **Quotes:** “HP referred to by card text is the current HP (printed/modified HP minus damage)”; “the compiled HP filter read printed HP instead of current HP (FAQ Q96)”
- **What is weak:** `view.stat()` evaluates `Stat.HP` as `max(0, hp - damage)`, the same as `REMAINING_HP` (verified in the code this session), so bindings whose only purpose is the HP fix (GD01-004, the ST01–04 `_CURRENT_HP_NOTE` cards, GD05-072, GD05-083) may be redundant, and the DSL has two stats with identical semantics.
- **Resolution plan:** Rust maps both stats to one implementation; for each HP-fix binding, compare its IR behaviour with the compiled script and retire it only when golden-identical (as W-63); the stale notes are listed for the Python side.
- **Verification** (unit): for each listed card, compiled and bound scripts give identical results on a set of HP/damage states; the card's tagged tests pass.
- **Status:** resolved: for each of the eight HP-fix binding cards, the Python compiler's script and the binding differ only in `Stat.HP` against `Stat.REMAINING_HP`. Rust maps both stats to one meaning and gets identical decisions and final states on HP and damage states. The cards are GD01-004, ST01-004, ST01-010, ST02-014, ST04-001, ST04-013, GD05-072 and GD05-083.
- **Post-acceptance:** retire the eight bindings and their stale notes (§12.11 answer (a)).
- **Evidence:**
  - `crates/gcg-engine/tests/p6k_behaviour.rs::w68_compiled_and_bound_hp_filters_agree_on_hp_damage_states`, with compiled scripts from `port/oracle/tests/vectors/p6k_hp_fragments.json`.
  - `pinned_cards_have_passing_rust_behaviour_tests`.
- **Ablation:** no AI effect (row K).

### W-69 — The registry build swallows binding errors; the card silently becomes unimplemented
- **Category:** effects · **Resolution type:** Rust fix (fail fast)
- **Sources:** `src/gcg_sim/effects/registry.py:325-339`, `tests/effects/test_card_coverage.py:22-29`
- **Quotes:** “except (ValueError, AssertionError, TypeError, KeyError, IndexError, AttributeError) as exc:”
- **What is weak:** a bug in a binding becomes a "binding error" entry that surfaces only when a game needs the card (`UnimplementedCardError`) or when the coverage test runs.
- **Resolution plan:** the Rust registry build returns an error listing every card that failed to load (bindings are code, so a failure is a build bug); "unimplemented" stays a separate, data-driven state for cards with no script.
- **Verification** (unit): a test registry with a deliberately broken binding fails to build with that card named; the packaged registry builds with zero errors.
- **Status:** resolved by construction: a script the loader cannot build fails `Registry::from_ir` with an error naming the card.
- **Evidence:** `crates/gcg-effects/tests/p6k_ir_scans.rs::w69_a_broken_binding_fails_the_build_naming_the_card`. The packaged IR counts 302 binding + 538 compiled + 313 vanilla = 1153 definitions (T-043).
- **Ablation:** no AI effect (row K).

### W-70 — Historical compiler and DSL bugs fixed in Python: regression risk for the port
- **Category:** effects · **Resolution type:** port-preserve + regression tests
- **Sources:** `102e72b`, `7c47b86`, `eccb538`, `eccb538:tests/rules/test_effects.py`, `b369dc8`, `c8cc8e9`, `f89c0cc`, `21489cb`, `9408681`, `0ee591d`, `aa0dd30`, `5c46d63`, `db94768`
- **Quotes:** “ENGINE: 'Then' part compiled outside the leading 'If' (GD03-072 discards without the condition)”; “Development cards not compiling (13-1-8-1/2)”; “start-phase freezes (EB01-001, ST14-001) use the stays-rested rule instead of phase conditions”; “fix fragile bindings, token-definition template, simultaneous battle damage resolution”
- **What is weak:** Then/If scoping (GD03-072, GD02-021), "Choose 2" playable with one target, "instead" losing "during this turn" (ST07-009) or replacing the damage step (GD03-109), multi-target pre-checks; cards that did not compile (ST03-011, GD03-085, GD05-113, 【Deploy･Development N】); missing DSL support for deployed-rested statics (Q354) and deploying a Pilot as a Unit (Q389/Q390); start-phase freezes (EB01-001, ST14-001) modelled as the stays-rested rule (Q208/Q288); fragile per-card bindings repaired at integration. The compiler is fragile on split or unusual lead sentences.
- **Resolution plan:** as W-57: every tagged test ported with its tag; the regression table covers these commits; golden records compared between implementations.
- **Verification** (unit + meta-test): ported tests pass; the meta-test covers each listed fix; Rust golden output equals Python's for every card.
- **Status:** resolved: as W-57. The W-70 rows of `docs/port/audit/regressions.json` all have passing traces, and the golden records equal Python's.
- **Evidence:** `crates/gcg-engine/tests/p6k_audit.rs::w57_w70_every_historical_fix_has_a_rust_test` and the golden check (T-042).
- **Ablation:** no AI effect (row K).

## Validator

### W-71 — The ST02/ST05 unmodified starter-deck exceptions are not applied
- **Category:** validator · **Resolution type:** Rust fix (deferred until data exists) + reporting
- **Sources:** `README.md:172-174`, `docs/DECK_FORMAT.md:69-72`, `docs/CONFLICTS.md:975-982`, `docs/research/OFFICIAL_SOURCES.md:88-94`, `docs/research/OFFICIAL_SOURCES.md:247-250`, `src/gcg_sim/deck/validate.py:17`, `src/gcg_sim/deck/validate.py:29-30`, `src/gcg_sim/data/official/banlist.json:15`, `src/gcg_sim/data/official/banlist.json:140`, `src/gcg_sim/data/official/banlist.json:327`, `data/sources.d/official.json`
- **Quotes:** “The validator cannot tell that a deck is an unmodified starter deck, so it reports them as violations and says so.”; “is not applied: the simulator cannot verify an unmodified starter deck”
- **What is weak:** the unmodified ST02 deck (3x ST02-016 Corsica Base) and ST05 deck (ST05-004 + ST05-009 vanilla pair) are tournament-legal officially but fail as RESTRICTED / ATTRIBUTE_PAIR; neither the official pages nor gcg-api contain the starter decklists (the product entries in `data/sources.d/official.json` say "no decklist"); support is planned only if an official decklist is added with provenance.
- **Resolution plan:** the Rust validator gets the exception logic behind a data hook: if the packaged data ever contains an official starter decklist with provenance, a deck whose multiset equals it is exempt from exactly the listed restriction; until then the validator's message keeps saying why it is reported. Adding the decklist is a data-refresh task outside the port.
- **Verification** (unit): with a synthetic decklist fixture, an exact ST02 list passes and a one-card variation fails; with the packaged data (no decklist), both are reported with the explanatory note as in Python.
- **Status:** resolved: the exception logic sits behind the data hook `official_product_decklists` (`crates/gcg-data/src/legality/lists.rs`, applied in `validate.rs`). With today's data the message is Python's. Adding the official ST02/ST05 decklists with provenance is a `/gcg-refresh-data` task: neither the official pages nor gcg-api has them.
- **Option location (A30):** `gcg-eval validate DECK`. `gcg-sim validate` is unchanged.
- **Evidence:**
  - `crates/gcg-data/tests/p6l_validator.rs::w71_starter_deck_exception_hook`: with the packaged data, Python's RESTRICTED note. With a synthetic decklist both exact lists are legal, a one-card variation fails, and the ST05 exception stays Python's.
  - `w71_decklists_need_provenance`.
  - e2e: `gcg-eval validate st02.txt` exits 1 with Python's note, and `gcg-sim validate` on the same file is byte-identical to Python's.
- **Ablation:** no AI effect (row L).

### W-72 — One current B&R list: no history and no region
- **Category:** validator · **Resolution type:** Rust fix (schema-ready, data unchanged)
- **Sources:** `docs/CONFLICTS.md:957-964`, `docs/research/OFFICIAL_SOURCES.md:96-102`, `docs/research/OFFICIAL_SOURCES.md:250-252`, `.claude/skills/gcg-refresh-data/references/sources.md:70-73`, `src/gcg_sim/data/official/banlist.json:20`, `src/gcg_sim/deck/official.py:112-127`
- **Quotes:** “The simulator validates against one list, not by date.”
- **What is weak:** only the list with effective date 2026-09-25 (the page date) exists; the US and Asia July lists took effect a day apart (2026-07-24 vs 07-25); a deck cannot be validated as of an earlier date or for a region, and each B&R update means hand-editing `banlist.json`.
- **Resolution plan:** the Rust validator's B&R model is a list of dated, region-tagged entries, of which the packaged data currently provides one; `--as-of DATE` and `--region` select an entry and fail clearly when no entry covers the request. Adding historical lists is a data-refresh task; the port does not change the packaged list.
- **Verification** (unit): with the packaged data, default validation equals Python's; with a two-entry fixture, `--as-of` before the second date uses the first list; a date before every entry is an error.
- **Status:** resolved: `BanlistHistory` holds dated, region-tagged lists. The packaged data has one, the current list, and with no options it is `validate_deck`'s. Adding the historical July lists is a data-refresh task; the port does not change `banlist.json`.
- **Option location (A30):** `gcg-eval validate DECK --as-of DATE --region R`. `gcg-sim validate` is unchanged.
- **Evidence:** in `crates/gcg-data/tests/p6l_validator.rs`:
  - `w72_default_check_equals_validate_deck_on_the_python_corpus`: 651 validations equal Python's row for row.
  - `w72_packaged_model_is_one_current_list`.
  - `w72_two_entry_fixture_selects_by_date`.
  - `w72_regional_lists_need_a_region_where_they_differ`.
  - `w72_malformed_histories_are_load_errors`.
  - e2e: `--as-of 2026-09-24` exits 1, naming the dates the data covers.
- **Ablation:** no AI effect (row L).

### W-73 — Vanilla banned pair: the predicate and the enumerated list can diverge, and divergence only adds a note
- **Category:** validator · **Resolution type:** data tripwire
- **Sources:** `docs/CONFLICTS.md:984-991`, `docs/research/OFFICIAL_SOURCES.md:243-244`, `.claude/skills/gcg-refresh-data/SKILL.md:171-186`, `src/gcg_sim/data/official/banlist.json:5`, `src/gcg_sim/deck/validate.py:202-218`
- **Quotes:** “so the list can lag a new set.”
- **What is weak:** membership is predicate OR the enumerated 22-card list; when they disagree the violation message adds a "Note:"; failing loudly on divergence depends on the refresh tooling, not on the validator.
- **Resolution plan:** a Rust data-gate test evaluates the predicate over the packaged pool and requires it to equal the enumerated list; the validator's runtime behaviour (predicate OR list, note on divergence) stays as in Python.
- **Verification** (unit + negative control): the test passes on the packaged data (22 matches); a synthetic vanilla Lv2/cost1/2AP/2HP Unit in a temporary data copy makes it fail.
- **Status:** resolved: a data-gate tripwire. The vanilla predicate's matches must equal the enumerated list and the data's own `predicate_matches_in_card_data`. The validator's behaviour is unchanged.
- **Evidence:** `crates/gcg-data/tests/p6l_tripwires.rs::w73_vanilla_predicate_equals_the_enumerated_list_with_negative_control`. The packaged data has 22 matches and 22 listed. Two negative controls: a synthetic vanilla `SY01-001`, and a copy that enumerates GD01-001.
- **Ablation:** no AI effect (row L).

### W-74 — Validator predicates that hold only for today's pool: "without effects", primary-name matching, one colour per card
- **Category:** validator · **Resolution type:** data tripwire
- **Sources:** `docs/CONFLICTS.md:993-1000`, `docs/research/OFFICIAL_SOURCES.md:245-246`, `docs/research/OFFICIAL_SOURCES.md:124-125`, `docs/research/DATA_INGESTION.md:220-221`, `src/gcg_sim/data/official/banlist.json:10`, `src/gcg_sim/deck/official.py:130-132`, `src/gcg_sim/deck/official.py:153-154`, `src/gcg_sim/deck/validate.py:133-151`
- **Quotes:** “'without effects' is not defined.”; “return str(value) in card.name”; “every UNIT, PILOT, COMMAND and BASE card number has exactly one color.”
- **What is weak:** "without effects" is read as effect text non-empty and not "-" (keywords count, a link alone does not); no current card is affected. The `name_contains` predicate tests only the primary name, not alias names (rule 2-2-4: GD02-098, ST12-011, ST12-012 and the 68 【Pilot】 Commands), so a future name-based B&R rule would miss alias holders (verified in the code this session). `check_colors` assumes one colour per card and skips colourless main-deck cards.
- **Resolution plan:** Rust validator keeps the same behaviour; tripwires fail the data gate when a card would be classified differently by the alternative reading (a Lv2/cost1/2AP/2HP Unit with only keywords or only a link), when a B&R entry uses `name_contains` with a value matching any alias of a card that its primary name does not, or when any card has more than one colour.
- **Verification** (unit + negative control): tripwires pass on the packaged pool; synthetic cards in a temporary data copy trigger each one.
- **Status:** resolved: three tripwires, with the validator's behaviour unchanged. They cover "without effects" reading, `name_contains` against aliases, and one colour per card. Correction to the entry's premise: the data has no `extra_names`. The rule 2-2-4 aliases are the 【Pilot】 names of 68 Commands and the "also treated as" text of GD02-098, ST12-011 and ST12-012.
- **Evidence:** in `crates/gcg-data/tests/p6l_tripwires.rs`, each with negative controls:
  - `w74_without_effects_reading_with_negative_controls`;
  - `w74_name_contains_misses_no_alias_with_negative_control`;
  - `w74_one_colour_per_card_with_negative_controls`.
- **Ablation:** no AI effect (row L).

### W-75 — The B&R list is enforced by default although the Comprehensive Rules never mention it
- **Category:** validator · **Resolution type:** port-preserve + reporting
- **Sources:** `docs/CONFLICTS.md:948-955`, `docs/DECK_FORMAT.md:87`, `src/gcg_sim/data/official/banlist.json:25`, `src/gcg_sim/deck/validate.py:268-289`, `src/gcg_sim/reports/summary.py:200-209`
- **Quotes:** “Enforce the B&R list by default, because benchmarks model tournament play”
- **What is weak:** `validate_deck(deck, banlist=False)` checks only the Comprehensive Rules (6-1-1-3 allows 4 copies of anything); the CLI always enforces B&R; only "standard constructed" is supported.
- **Resolution plan:** keep the default (tournament play). The Rust CLI exposes the existing library switch as `--no-banlist`. Default reports already state the list's date (`versions.banlist_effective_date` in `results.json`, and the Versions section of `summary.md`, `summary.py:200-209`), and that stays byte-identical. The format applied (Constructed, the only one supported) and whether a list was applied go into `analysis.json` (written only with `--analysis`, W-90). A `--no-banlist` run is opt-in and has no Python CLI equivalent, so only that run's report header carries a warning that no B&R list was applied.
- **Verification** (unit + e2e): default CLI validation and default reports equal Python's. `--no-banlist` accepts a 4-copy deck of a banned card, and that run's report header carries the warning. `analysis.json` names the format and the list date.
- **Status:** resolved: the default stays tournament play, with the B&R list applied. `--no-banlist` accepts banned cards, and only that run's report header carries the warning. Every `gcg-eval validate` report names the format and the list applied. `gcg-eval benchmark --analysis` now writes the validator's `scope` block into `analysis.json` (`crates/gcg-eval/src/benchmark_opts.rs` `analysis_scope`; `docs/port/eval/weakness-status/GAP-C.md`): `format` (Constructed), `banlist {applied, effective_date, regions, format}`, `unsupported_formats`, `unchecked_tournament_rules`, `unmodelled_tournament_procedures` and `edition_beta`.
- **Option location (A30):** `gcg-eval validate DECK --no-banlist`, and `gcg-eval benchmark --analysis` for the `analysis.json` block. `gcg-sim validate` and the benchmark reports are unchanged.
- **Evidence:**
  - `crates/gcg-data/tests/p6l_validator.rs::w75_no_banlist_accepts_a_banned_card`: 4x GD01-020 is BANNED by default and legal with no list.
  - `validate_cmd::tests::w75_no_banlist_warns_in_the_header`.
  - `validate_cmd::tests::embedded_sources_and_the_analysis_scope_block`: the block's contents.
  - `crates/gcg-eval/tests/gapc_tools.rs::w75_w76_w104_analysis_scope_block_and_timing_fields` (pass): two `gcg-eval benchmark` runs, with and without `--analysis`. The block names Constructed, `applied: true`, the effective date equal to `results.json`'s `versions.banlist_effective_date` (2026-09-25) and the regions. Every other report file of the `--analysis` run equals the plain run's byte for byte, and the plain run's bytes equal `gcg-sim benchmark`'s (T-153).
  - e2e: Python and Rust `gcg-sim validate` give identical stdout, stderr and exit codes on 8 decks.
- **Ablation:** no AI effect (row L).

### W-76 — Tournament-only deck rules, limited formats, tournament procedures and Edition Beta legality are not modelled
- **Category:** validator · **Resolution type:** reporting (explicit scope, additive outputs)
- **Sources:** `docs/research/OFFICIAL_SOURCES.md:126-128`, `docs/research/OFFICIAL_SOURCES.md:191-198`, `docs/research/OFFICIAL_SOURCES.md:199-210`, `docs/research/OFFICIAL_SOURCES.md:233-236`, `docs/DECK_FORMAT.md:72-73`, `docs/CONFLICTS.md:31-32`, `src/gcg_sim/data/official/deck_construction.json:158-161`, `src/gcg_sim/data/official/edition_language.json:281`, `docs/research/OFFICIAL_SOURCES.md:48-51`, `data/official_raw/en_pdf_floor_rule_en.txt:571-581`, `src/gcg_sim/data/official/bo3_match_rules.json:16-21`, `docs/CONFLICTS.md:948-955`, `src/gcg_sim/reports/summary.py:67-226`, `src/gcg_sim/reports/results.py:28-38`
- **Quotes:** “the 7-day ban on newly released card numbers at Premier events;”; “No official statement on tournament legality was found”; “7-day new-card rule for Premier events (TRM 2.4.1; Sanctioned Floor Rules Section 5)”; “will construct a legal deck from that product only.”; “"intentional_draws_concessions_penalties": "Not modelled.",”
- **What is weak:** the following are not modelled: the 7-day new-card ban at Premier events (and the Asia Newtype Challenge rule), sleeves, card condition, deck lists, language and region rules, and judge deck checks (replaced by pre-match validation). Edition Beta printings are validated by card number only (RP-001 and EXBP-001 exist only in Edition Beta), and their tournament legality is undecided officially. Only the Constructed format is supported (W-75). The floor rules also define the limited formats Sealed ("Players will receive specific event product … and will construct a legal deck from that product only") and Draft. Sealed Battle is a separate RULES-tagged notice, listed apart from Team Battle and Battle Royale. These are two-player deck-construction formats, not multiplayer (W-131), and the explicit design decisions do not cover them. Intentional draws, concessions and penalties are tournament procedures, recorded as "Not modelled." in `bo3_match_rules.json`. They are not part of the clock (W-133).
- **Resolution plan:** the Rust validator adds an optional `--event-date DATE` check for the 7-day rule, using the release dates already in the packaged data (no data change; skipped with a note when a date is missing). An opt-in `gcg-sim validate --scope` lists the tournament rules the validator does not check and flags Edition Beta-only printings. It states the deck-construction format applied (Constructed), and lists Sealed and Draft as unsupported formats. Without the flag, `validate` prints what Python prints. Supporting Sealed would need its rules page (`news/sealed-battle.html`, linked from the Rules page but not stored) to be added through `/gcg-refresh-data`; that is a data task outside the port, handed to the lead. The byte-compared benchmark outputs stay unchanged: `summary.md` has no scope section (its sections run from Result to Notes, `summary.py:67-226`), and the `results.json` notes are fixed text (`results.py:28-38`). So the unmodelled tournament procedures (intentional draws, concessions, penalties) are listed in a `scope` block of `analysis.json` (written only with `--analysis`, W-90) and in the Rust report documentation.
- **Verification** (unit + e2e): a deck containing a card released 3 days before `--event-date` fails the optional check and passes without it. `validate --scope` lists the unchecked rules, flags RP-001, and names Constructed as the applied format with Sealed and Draft as unsupported, and `validate` without the flag prints what Python prints. `analysis.json`'s scope block lists the unmodelled tournament procedures, and the benchmark outputs of the same run without `--analysis` are byte-identical to Python's.
- **Status:** resolved: `--event-date` checks the 7-day new-card rule (TRM 2.4.1), and `--scope` lists:
  - the applied format, Constructed;
  - Sealed and Draft as unsupported;
  - the unchecked tournament rules;
  - the unmodelled tournament procedures;
  - the Edition Beta facts, flagging RP-001.

  `gcg-sim validate` is unchanged. The same scope block is now written into `analysis.json` by `gcg-eval benchmark --analysis` (W-75; `docs/port/eval/weakness-status/GAP-C.md`), with the unmodelled tournament procedures of `bo3_match_rules.json`. Supporting Sealed needs its rules page in the data through `/gcg-refresh-data`, a lead task outside the verification.
- **Option location (A30):** `gcg-eval validate DECK --event-date DATE --scope`, and `gcg-eval benchmark --analysis` for the block. The entry's plan named `gcg-sim validate --scope`.
- **Evidence:**
  - In `crates/gcg-data/tests/p6l_validator.rs`: `w76_event_date_seven_day_rule` (2x ST11-016 fails 3 days after release and passes without the option), `w76_release_dates_cover_every_main_deck_set` (GD06 dated 2026-10-30) and `w76_scope_statement`.
  - `validate_cmd::tests::w76_*`.
  - e2e: `[NEW_CARD] ST11-016 ...`, exit 1; `--scope` on RP-001 prints the Edition Beta flag.
  - `crates/gcg-eval/tests/gapc_tools.rs::w75_w76_w104_analysis_scope_block_and_timing_fields` (pass): the `analysis.json` block's procedures equal `bo3_match_rules.json`'s, its unsupported formats are Sealed and Draft, it lists the 7-day rule and flags RP-001. The outputs of the same run without `--analysis` are byte-identical to `gcg-sim benchmark`'s (T-153, `p6m_cli.rs::t153_benchmark_writes_gcg_sims_bytes_and_ignores_the_factory_variable`).
- **Ablation:** no AI effect (row L).

### W-77 — Deck file format limits
- **Category:** validator · **Resolution type:** Rust fix (diagnostics)
- **Sources:** `docs/DECK_FORMAT.md:22-26`
- **Quotes:** “There are no end-of-line comments, because card names may contain `#`.”
- **What is weak:** no end-of-line comments; listing an EX Resource (a token) fails CARD_TYPE and still counts toward RESOURCE_SIZE.
- **Resolution plan:** the Rust parser keeps the format and the violations (equivalence) and adds a hint to the CARD_TYPE message when the card is an EX token and to lines that look like "name  # comment" whose name does not resolve.
- **Verification** (unit): violation codes and counts equal Python's on the deck-format test corpus; the hints appear in the two cases.
- **Status:** resolved: the parser keeps the format and the violations. Hints are additive: EX Resource or EX Base lines, and `#` after a valid entry.
- **Option location (A30):** the hints are printed by `gcg-eval validate`. `gcg-sim validate`'s output is unchanged.
- **Evidence:**
  - `w72_default_check_equals_validate_deck_on_the_python_corpus`: codes and messages equal Python's.
  - `crates/gcg-data/tests/p6l_validator.rs::w77_ex_token_hints` and `w77_comment_hints`.
  - e2e: `gcg-eval validate hash.txt` prints the hint.
- **Ablation:** no AI effect (row L).

### W-78 — Legality depends on simulator support (UNIMPLEMENTED)
- **Category:** validator · **Resolution type:** port-preserve + reporting
- **Sources:** `docs/DECK_FORMAT.md:67`, `docs/DECK_FORMAT.md:88`, `src/gcg_sim/deck/validate.py:268`
- **Quotes:** “Every card has an implemented effect in this simulator version.”
- **What is weak:** a new card missing from the registry makes a legal deck fail; `validate_deck(implemented=False)` skips the check.
- **Resolution plan:** keep the check (a benchmark cannot play an unimplemented card); the Rust report separates "illegal by the rules" from "not playable by this simulator version", and the CLI exposes the existing switch for validation-only use.
- **Verification** (unit): a deck with a synthetic unimplemented card reports UNIMPLEMENTED under the simulator-support heading and passes rules validation with the switch.
- **Status:** resolved: the check stays. The report separates the rules (Comprehensive Rules and the B&R list) from simulator support. There is one addition to A30's option list, `--no-implemented`, which W-78's plan needs on the CLI.
- **Option location (A30):** `gcg-eval validate DECK --no-implemented`.
- **Evidence:** `crates/gcg-data/tests/p6l_validator.rs::w78_simulator_support_is_reported_apart` and `validate_cmd::tests::w78_simulator_support_heading`.
- **Ablation:** no AI effect (row L).

## Data

### W-79 — Official errata missing from gcg-api are applied by hand (ST12-001)
- **Category:** data · **Resolution type:** data tripwire (data unchanged)
- **Sources:** `docs/CONFLICTS.md:910-920`, `docs/research/OFFICIAL_SOURCES.md:212-214`, `docs/research/OFFICIAL_SOURCES.md:262-264`, `docs/research/DATA_INGESTION.md:206-210`, `.claude/skills/gcg-refresh-data/references/conflict-policies.md:80-82`, `src/gcg_sim/sources/conflicts.py:1603`, `data/sources.d/official.json`, `data/conflicts.json`, `8182f98`
- **Quotes:** “The pinned gcg-api snapshot (errata.json and cards.ndjson) still has the printed wording.”; “official errata (2026-09-04) adds 【Once per Turn】; missing from gcg-api”; “Apply official ST12-001 errata (news 02_193) through a curated conflict and field override”
- **What is weak:** the 2026-09-04 Epyon erratum is in neither `errata.json` nor `cards.ndjson`, and Bandai does not update card pages for errata; it is applied through an `overrides.json` field override (major severity), so every future erratum needs manual curation.
- **Resolution plan:** Rust reads the override as packaged. A data-gate test cross-checks every erratum recorded in the official-source data against the effective card text and fails if an erratum is neither in the gcg-api text nor covered by an override.
- **Verification** (unit + negative control): ST12-001's effective text has 【Once per Turn】; removing the override in a temporary copy fails the test.
- **Status:** resolved: a tripwire parses every errata notice of the official-source data: GD04-067, ST12-001 and T-013. It requires the corrected wording in the effective card, plus gcg-api's applied erratum or an `errata_supersedes_print` override.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w79_errata_are_applied_with_negative_control`. Removing ST12-001's override in a copy fails with two findings.
- **Ablation:** no AI effect (row K).

### W-80 — Card text format defects corrected by overrides (GD02-053, GD03-015, GD03-035)
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `docs/CONFLICTS.md:1050-1086`, `docs/research/DATA_INGESTION.md:263-264`, `docs/research/EFFECT_TEMPLATES.md:95-97`, `scripts/cluster_effects.py:76-77`, `data/conflicts.json`
- **Quotes:** “so keyword_effects omits them and a template compiler would miss the keyword.”
- **What is weak:** GD02-053 prints [Suppression] in square brackets (major), and GD03-015 and GD03-035 use an ASCII ':' as the cost separator; a compiler must normalise them (rule N06) or it drops the keyword.
- **Resolution plan:** a data-gate scan of all card text for bracketed keyword names and ASCII cost separators; every hit must be covered by a normalisation rule or override.
- **Verification** (unit): the scan finds exactly the three known cards, each covered; GD02-053 has <Suppression> in Rust.
- **Status:** resolved: a scan of every printing for keywords outside `<>` and for ASCII cost separators. Each hit must be covered by a `text_format_correction` override.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w80_text_format_defects_are_exactly_the_known_cards_and_covered` (exactly GD02-053, GD03-015 and GD03-035; a synthetic card is found) and `crates/gcg-effects/tests/p6k_ir_scans.rs::w80_gd02_053_has_suppression_in_rust`.
- **Ablation:** no AI effect (row K).

### W-81 — gcg-api structure fields (keyword_effects, timing_markers, link_refs) are lossy
- **Category:** data · **Resolution type:** port-preserve + test
- **Sources:** `docs/CONFLICTS.md:1005-1048`, `docs/research/DATA_INGESTION.md:252-254`, `docs/research/DATA_INGESTION.md:258-272`, `docs/research/EFFECT_TEMPLATES.md:98-104`, `src/gcg_sim/sources/conflicts.py:1004-1015`, `data/conflicts.json`
- **Quotes:** “The gcg-api extractor is lossy by design”; “【Pilot】 sections are not recorded in timing_markers”; “It is a mention list and should not be read as the card's own keywords.”
- **What is weak:** `keyword_effects` lists keywords a card only mentions or grants (GD04-067 lists all seven); `timing_markers` drops 【Pilot】, Development N and pilot qualifiers and splits Activate･Main; `link_refs` loses the name/trait distinction and adds a spurious "Machu" on 14 printings. Abilities must come from the text (5 conflicts use `engine_derives_markers`).
- **Resolution plan:** the Rust data model never reads these fields for behaviour; a test asserts no engine or AI code path depends on them.
- **Verification** (unit): a test build with the three fields blanked in a temporary data copy produces identical registry content ids and golden records.
- **Status:** resolved: no Rust path reads `keyword_effects`, `timing_markers` or `link_refs`.
- **Evidence:**
  - `crates/gcg-effects/tests/p6k_ir_scans.rs::w81_blanking_keyword_effects_timing_markers_and_link_refs_changes_nothing`: identical fingerprints, golden records and content ids.
  - Python: `port/oracle/tests/test_p6k_blank_fields.py`.
- **Ablation:** no AI effect (row K).

### W-82 — A null printed AP is read as 0 (inference); EX Base AP is null in the source
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `docs/research/DATA_INGESTION.md:226-238`, `docs/CONFLICTS.md:1091-1120`, `data/conflicts.json`, `6078099`
- **Quotes:** “a zero AP arrives the same way (inferred, not stated upstream)”; “EXBP-035 added to data-null:EX-BASE:ap”
- **What is weak:** gcg-api maps a printed "-" and a zero AP both to null; 109 Base, 33 EX Base and 13 Unit printings (10 Unit numbers, plus tokens T-012/T-014) have null AP, read as 0; EX Base AP is filled through a conflict resolution.
- **Resolution plan:** keep the reading; a tripwire lists every null-AP printing and fails when a new one appears that is not a Base, EX Base or a reviewed Unit/token, so each new case is reviewed.
- **Verification** (unit + negative control): the list on the packaged data equals the reviewed set; a synthetic null-AP Unit fails it.
- **Status:** resolved: a reviewed tripwire on null printed AP.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w82_null_ap_printings_are_reviewed_with_negative_control`. It covers 109 Base, 33 EX Base, 13 Unit and 3 Unit-token printings; a synthetic null-AP Unit fails.
- **Ablation:** no AI effect (row K).

### W-83 — Edition Beta stat records are defective (one-row shift in the source listing)
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `docs/research/DATA_INGESTION.md:185-196`, `src/gcg_sim/data/overrides.json:228-231`
- **Quotes:** “The Edition Beta stat values are not real Beta stats.”; “That points to a one-row shift in the source listing”
- **What is weak:** T-002_p1, T-003_p1, T-006_p1 and R-001_p4 (a Resource with AP3/HP1) copy the previous record's stats, T-001_p1 has null AP/HP; curated overrides pick non-Beta canonical printings (e.g. R-001_p7). A new Beta-like printing needs the same handling.
- **Resolution plan:** a tripwire flags any printing whose stats equal the previous record's in source order while differing from its canonical printing, and any Resource with AP/HP; each hit must be covered by a canonical-printing override.
- **Verification** (unit): the tripwire finds exactly the known printings, all covered.
- **Status:** resolved: every printing whose AP or HP deviates from its canonical printing, and every Resource with stats, must be an Edition Beta record superseded by a `divergent:` resolution.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w83_edition_beta_stat_records_are_all_superseded`. It finds exactly R-001_p4, T-001_p1, T-002_p1, T-003_p1 and T-006_p1. Removing `divergent:T-002` uncovers T-002_p1.
- **Ablation:** no AI effect (row K).

### W-84 — Link, name and trait anomalies in the card data
- **Category:** data · **Resolution type:** port-preserve + test
- **Sources:** `docs/research/DATA_INGESTION.md:131-133`, `docs/research/DATA_INGESTION.md:190`, `docs/research/DATA_INGESTION.md:247-256`
- **Quotes:** “`[Ericht Samaya]` (GD05-026) matches no pilot in the snapshot”
- **What is weak:** the GD05-026 link alternative names no existing Pilot; GD01-051_p1 has the malformed link "Trait [Enhanced Human]"; "(Dianna Counter)" is a trait no card carries yet; parsers must tolerate them.
- **Resolution plan:** the Rust parser tolerates them exactly as Python does; a data-gate report lists link targets and traits that match nothing, so new anomalies are reviewed.
- **Verification** (unit): the report on the packaged data equals the known anomalies; GD05-026 links as in Python.
- **Status:** resolved: a tripwire of the known link, name and trait anomalies.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w84_link_and_trait_anomalies_equal_the_known_set` and `crates/gcg-data/tests/carddefs.rs::every_card_def_equals_python_field_by_field` (T-035).
- **Ablation:** no AI effect (row K).

### W-85 — Derived data files are curated by hand; curated conflict texts drifted from their resolutions
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `CLAUDE.md:34-43`, `.claude/skills/gcg-refresh-data/SKILL.md:193-197`, `32e9f84`
- **Quotes:** “Derived files are curated by hand”; “curated conflict texts resynced with resolutions”
- **What is weak:** `official/*.json` (verbatim quotes), `rules_na.json`, `overrides.json` and `curated_conflicts.json` are hand-curated; quotes are checked and hashes re-locked, but nothing structural keeps `curated_conflicts.json` in sync with `overrides.json`.
- **Resolution plan:** a Rust data-gate test joins `curated_conflicts.json` and `overrides.json` by conflict id and fails on any resolution text or policy mismatch, on orphaned entries on either side, and on hash drift against the lock.
- **Verification** (unit + negative control): passes on the packaged data; editing one resolution text in a temporary copy fails it.
- **Status:** resolved: the curated conflicts, the overrides and the lock must agree.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w85_curated_conflicts_overrides_and_lock_agree_with_negative_control`. 333 curated conflicts are resolved and equal to the report. An edited decision text and an orphaned resolution each fail.
- **Ablation:** no AI effect (row K).

### W-86 — Printing text divergences resolved to the base printing; a wrong rule citation was fixed in review
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `6078099`
- **Quotes:** “8 new printing divergences resolved to the base printing”; “divergent:RP-068: drop the claim attributed to rule 3-6, which only defines Resources.”
- **What is weak:** alternate printings whose text differs are treated as the base printing (7 reminder-only ST12/ST13 prints, RP-068_p1 "-"); a rationale once cited the wrong rule.
- **Resolution plan:** a tripwire diffs each alternate printing's normalised text (reminder text stripped) against its base printing and fails on any difference not covered by a `canonical_printing` conflict.
- **Verification** (unit): the divergences found equal the curated set.
- **Status:** resolved: printing divergences must equal the curated set.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w86_printing_divergences_equal_the_curated_set`. 20 printings of 13 card numbers are each covered by a `canonical_printing` resolution; a synthetic divergent printing is found.
- **Ablation:** no AI effect (row K).

### W-87 — The Comprehensive Rules file has wrong cross-references and typos (the source is never edited)
- **Category:** data · **Resolution type:** port-preserve + test
- **Sources:** `docs/research/RULES_INDEX.md:41-51`, `docs/research/RULES_INDEX.md:160-201`, `docs/CONFLICTS.md:1122`, `docs/CONFLICTS.md:1257-1269`
- **Quotes:** “Wrong cross-references (11 of the 34 references in the file)”; “Ten references still use the old numbers”
- **What is weak:** left over from the Ver. 1.1.0 renumbering (e.g. 2-7-1 "See 7. Attacking and Battles", 8-6-2 "See 6-5", 13-2-1-1 "see 9-1-7"), plus typos (11-3-1 "becomes zero less", 5-20-2, 13-2-11-1 "is set"); "exile" is undefined. Traceability cites the intended rule.
- **Resolution plan:** the Rust rules index loads the unmodified file and the same `rules_xref_intended_target` mapping; a test asserts every cross-reference resolves either directly or through the mapping.
- **Verification** (unit): 34 references, 11 mapped, 0 unresolved.
- **Status:** resolved: every rules cross-reference resolves. The detector finds 32 references in the 1.9.0 file; 11 are broken, and all 11 are mapped by a `rules_xref_intended_target` resolution. The entry's "34" counts an older text.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w87_every_rules_cross_reference_resolves`.
- **Ablation:** no AI effect (row K).

### W-88 — Provenance gaps: some official sources were read but not stored; cards.json is omitted from the package
- **Category:** data · **Resolution type:** reporting (provenance listing)
- **Sources:** `docs/research/OFFICIAL_SOURCES.md:42-46`, `docs/research/OFFICIAL_SOURCES.md:230-232`, `.claude/skills/gcg-refresh-data/references/sources.md:65-68`, `docs/SOURCES.md:20-22`, `docs/SOURCES.md:35`, `docs/ASSUMPTIONS.md:18-22`, `data/sources.d/official.json`, `data/sources.d/rules.json`
- **Quotes:** “That PDF is larger than 3 MB, so only its text extraction is kept.”; “are omitted from the package to keep the wheel small”; “PDF is 3,046,969 bytes (>= 3 MB), so it is not stored”
- **What is weak:** only the hash and text of the Asia floor-rules PDF are kept; news listing pages 2–14, four event pages, the decks page and the JP Newtype Challenge PDF were not retained; the rules markdown omits the PDF's revision history; `cards.json` and `cards/en/*.json` are omitted (equivalence with `cards.ndjson` is checked at ingestion); `data/` has no README.
- **Resolution plan:** the Rust data crate embeds the provenance fragments and exposes `gcg-sim data status --sources`, listing every source with its hash and whether its content is packaged, text-only or hash-only. Storing more raw sources is a data-refresh decision outside the port.
- **Verification** (e2e): the listing covers every entry of `data/sources.d/*.json` and flags the Asia PDF as text-only.
- **Status:** resolved: every source of `data/sources.d/*.json` is classed as packaged, text-only or hash-only (`docs/port/audit/data-provenance.md`). `gcg-eval data-status --sources` (`crates/gcg-eval/src/data_status.rs`; `docs/port/eval/weakness-status/GAP-C.md`) lists every source with its SHA-256, its class and its location. It embeds `data/SOURCES.lock.json`, the checked aggregate of the fragments, which the code digest already covers. Deviation from the plan: the listing lives in `gcg-eval`, not `gcg-data`, because `gcg-data` embeds exactly the Python runtime's files (PLAN §2.4).
- **Option location (A30):** `gcg-eval data-status --sources`. The entry's plan named `gcg-sim data status --sources`, and `gcg-sim` stays unchanged.
- **Evidence:**
  - `crates/gcg-data/tests/p6k_data_tripwires.rs::w88_provenance_listing_covers_every_source`. 49 sources equal the lock's, only the Asia floor-rules PDF is text-only, and every packaged file's SHA-256 equals the lock's.
  - `crates/gcg-eval/tests/gapc_tools.rs::w88_data_status_sources_lists_every_source_and_flags_the_asia_pdf_text_only` (pass): the listing covers all 49 entries with the fragment's SHA-256, each class equals P6-K's file-based class, and the only text-only source is `official-asia-floor-pdf`.
  - `target/release/gcg-eval data-status --sources` (run 2026-11-04): "sources: 49 in data/SOURCES.lock.json (43 packaged, 1 text-only, 5 hash-only)", and the line `text-only official-asia-floor-pdf`.
- **Ablation:** no AI effect (row K).

### W-89 — dataset_version's sha is the code commit that produced the data, not the data commit
- **Category:** data · **Resolution type:** port-preserve + docs
- **Sources:** `.claude/skills/gcg-refresh-data/references/sources.md:35-36`, `docs/research/DATA_INGESTION.md:23`, `docs/REPORTS.md:72`
- **Quotes:** “`dataset_version`'s sha is the code commit that produced the data, not the data commit that the lock pins.”
- **What is weak:** a provenance subtlety that `results.json` versions must keep and that readers easily misread.
- **Resolution plan:** Rust reports reproduce the field exactly (equivalence) and the Rust report documentation explains it next to the lock's data commit.
- **Verification** (unit): `versions.dataset_version` equals Python's for the packaged data.
- **Status:** resolved: `dataset_version` names the code commit that produced the data, documented in `docs/port/audit/data-provenance.md`. The Phase 8 Rust report docs should repeat it.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w89_dataset_version_names_the_code_commit_not_the_data_commit` and `crates/gcg-data/tests/status.rs::versions_equal_python` (T-039).
- **Ablation:** no AI effect (row K).

## Reporting

### W-90 — Per-card and benchmark-card statistics are correlations inside AI-vs-AI games
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `docs/REPORTS.md:80`, `docs/REPORTS.md:100-102`, `.claude/skills/gcg-benchmark/SKILL.md:63-69`, `src/gcg_sim/reports/hypotheses.py:1-8`, `src/gcg_sim/reports/hypotheses.py:92`, `src/gcg_sim/reports/results.py:28-37`, `README.md:167-169`
- **Quotes:** “These are correlations inside AI-vs-AI games.”; “Correlation only: longer games also draw more cards.”; “Both seats are played by the same AI”
- **What is weak:** "win rate when drawn" and "benchmark card seen" signals are associations; cards drawn in longer games look better or worse for unrelated reasons (length bias); `loss_rate_lift` is association only; hypotheses are things to test, not conclusions.
- **Resolution plan:** keep the existing statistics byte-identical (equivalence) and add, in a separate `analysis.json` written only with `--analysis`, length-stratified per-card rates (strata by game length in turns, Mantel-Haenszel pooled difference) and, for the top hypotheses, a ready-to-run paired card-swap command (W-42) that turns the correlation into an intervention test.
- **Verification** (unit + e2e): unit test on a synthetic game set where a card's raw lift is entirely due to length: the stratified estimate is about zero; e2e: reports without `--analysis` are byte-identical to Python's.
- **Status:** resolved: `analysis.json` adds `cards_by_length`, a Mantel-Haenszel risk difference within game-length strata, and `swap_tests`, a variant deck per card hypothesis with its `gcg-sim benchmark` and `gcg-eval compare-runs` commands. The reference statistics are unchanged.
- **Option location (A30):** `gcg-eval benchmark --analysis`.
- **Evidence:**
  - `crates/gcg-runner/tests/p6m_analysis.rs::w90_a_pure_length_effect_vanishes_within_length_strata` (3,000 synthetic games) and `w90_card_hypotheses_get_a_paired_swap_test`.
  - `crates/gcg-eval/tests/p6m_cli.rs::opt_ins_add_files_and_keep_the_reports`.
- **Ablation:** no AI effect (row M).

### W-91 — Results are specific to the AI and preset, and not comparable across AI versions
- **Category:** reporting · **Resolution type:** Rust fix (guard in comparison tools)
- **Sources:** `docs/REPORTS.md:17`, `docs/REPORTS.md:63-65`, `src/gcg_sim/reports/results.py:31-32`, `.claude/skills/gcg-benchmark/SKILL.md:96-102`
- **Quotes:** “reports made before the October 2026 AI update (schema 1.0.0) are not comparable with new ones.”
- **What is weak:** both seats play the same AI, so results measure the decks under that AI, not under human play; comparisons need the same preset and `code_digest`; the new `standard` beats the old one 58% of the time; the Rust strengthened presets will again differ from the reference.
- **Resolution plan:** Rust `results.json` records the AI identity (implementation, preset, and for gcg-eval runs the reference or strengthened search), using the existing fields where they exist; every Rust comparison tool (`compare-runs`, gcg-eval) refuses to pair or pool runs whose AI identity or data version differ unless `--allow-mixed` is given, and prints why.
- **Verification** (unit): comparing a strengthened-`standard` run with a reference-`standard` run fails with a clear message; identical identities pass.
- **Status:** resolved: `gcg-eval compare-runs` refuses two runs whose AI identity or `versions` differ. `--allow-mixed` compares them anyway and prints the differences.
- **Option location (A30):** `gcg-eval compare-runs --allow-mixed`.
- **Evidence:** `crates/gcg-runner/tests/p6m_reports.rs::w91_runs_of_different_ais_are_not_compared` and `crates/gcg-eval/tests/p6m_cli.rs::w91_compare_runs_refuses_runs_of_different_ais`.
- **Ablation:** no AI effect (row M).

### W-92 — Game-level intervals and tests treat games within a BO3 match as independent
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `.claude/skills/gcg-benchmark/references/statistics.md:25-30`, `docs/REPORTS.md:66-68`, `src/gcg_sim/reports/aggregate.py:28-44`, `src/gcg_sim/reports/stats.py:12-25`, `src/gcg_sim/reports/stats.py:50-60`, `src/gcg_sim/reports/results.py:28-30`
- **Quotes:** “Games within a match are correlated (same decks, loser chooses first player)”; “`ci95` is a 95% Wilson score interval”
- **What is weak:** games 2–3 of a match share decks and the chooser depends on the previous game, yet game win rates, splits and per-card tests use plain Wilson intervals and pooled two-proportion tests over games, so their precision is overstated; the skill uses game counts for sample-size planning.
- **Resolution plan:** keep the existing game-level fields; `analysis.json` adds match-clustered intervals for game-level rates (cluster bootstrap over matches with a seeded RNG, plus the design effect), and the Rust port of the benchmark skill's sample-size guidance uses the design effect.
- **Verification** (unit): on simulated BO3 data with known within-match correlation, the clustered interval covers the true rate at about 95% over 1,000 replications while the plain interval under-covers.
- **Status:** resolved: `analysis.json` `clustered` gives a match-clustered interval, a seeded cluster bootstrap over matches and the design effect, beside the Wilson interval. Follow-up (Phase 8, lead-owned): the benchmark skill's sample-size guidance should use the design effect.
- **Option location (A30):** `gcg-eval benchmark --analysis`.
- **Evidence:** `crates/gcg-runner/tests/p6m_analysis.rs::w92_match_clustered_intervals_cover_and_plain_ones_do_not`. Over 1,000 replications of 100 BO3-like matches with within-match correlation 0.36, clustered coverage is 0.935 and plain Wilson 0.837.
- **Ablation:** no AI effect (row M).

### W-93 — Draws count as non-wins
- **Category:** reporting · **Resolution type:** port-preserve + reporting
- **Sources:** `docs/REPORTS.md:68`, `docs/python/AI.md:311`, `docs/python/AI.md:326`, `.claude/skills/gcg-benchmark/references/statistics.md:14`, `src/gcg_sim/reports/summary.py:36-40`, `15ae2eb`, `ef05730`
- **Quotes:** “Draws count as non-wins and are reported separately.”; “remove unused draw-as-half helpers”
- **What is weak:** the wins-only metric (adopted in ef05730; draw-as-half helpers removed in 15ae2eb) understates a deck that draws often, and the AI tuning validation still scores draws as 0.5 (W-40), so metrics are inconsistent across tools.
- **Resolution plan:** keep wins-only in every reference-equivalent output. All Rust tools (gcg-eval included) use the same wins-only definition from one shared function. The draw rate is printed next to the win rate only in gcg-eval output and in `analysis.json` (W-90). `summary.md` is byte-compared, so it keeps Python's Draws count column and prints no draw rate (`summary.py:36-40`).
- **Verification** (unit): a test asserts every rate-producing tool calls the shared function. For a run with draws, gcg-eval output and `analysis.json` print the draw count and the draw rate, and `summary.md` is byte-identical to Python's.
- **Status:** resolved: every rate in the workspace comes from one wins-only Wilson interval (`gcg_runner::reports::stats::wilson`). `analysis.json` and `gcg-eval` print the draw count and rate beside each rate, and `summary.md` and `results.json` are unchanged.
- **Evidence:** `crates/gcg-runner/tests/p6m_reports.rs::w93_every_rate_tool_uses_the_one_wins_only_interval` and `crates/gcg-runner/tests/p6m_analysis.rs::w93_draws_are_non_wins_with_their_rate_beside`.
- **Ablation:** no AI effect (row M).

### W-94 — Hypothesis error control is per kind of comparison; thresholds are hand-set
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `docs/REPORTS.md:81`, `.claude/skills/gcg-benchmark/SKILL.md:68-69`, `src/gcg_sim/reports/hypotheses.py:5-7`, `src/gcg_sim/reports/hypotheses.py:54-65`, `src/gcg_sim/reports/hypotheses.py:199-205`, `ef05730`
- **Quotes:** “at most ``ALPHA`` per kind of comparison”; “Never act on a signal backed”; “noise-gated hypotheses (z-test + Holm)”
- **What is weak:** Holm runs separately for card deltas, benchmark threats, play/draw (a family of 1) and redraw (a family of 1, `MIN_REDRAWS = 10`), so the chance of at least one spurious hypothesis per run is up to about four times alpha; `MIN_GAMES 20`, `MIN_DELTA 0.05` and `MIN_SPLIT_DELTA 0.10` are hand-set; the skill advises never acting on fewer than about 50 observations.
- **Resolution plan:** keep the existing hypotheses byte-identical; `analysis.json` adds a run-level Holm adjustment across all families and the adjusted p-value of each hypothesis, so a reader can apply run-level control.
- **Verification** (unit): simulated null runs (no real effect) give a run-level false-positive rate at or below alpha with the run-level adjustment, and up to about 4 alpha without it.
- **Status:** resolved: `analysis.json` `hypotheses` gives every family comparison with its rounded and unrounded p-values. It adds the reference decision, Holm per family, and a run-level Holm adjustment.
- **Option location (A30):** `gcg-eval benchmark --analysis`.
- **Evidence:** `crates/gcg-runner/tests/p6m_analysis.rs::w94_run_level_holm_keeps_the_false_positive_rate_at_alpha`. Over 400 null runs, per-family control gives at least one spurious hypothesis in 16.75% of runs; the run-level adjustment gives 2.0%.
- **Ablation:** no AI effect (row M).

### W-95 — "Never played" signals have no significance test or cap, and cost discards count as never played
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `src/gcg_sim/reports/hypotheses.py:7-8`, `src/gcg_sim/reports/hypotheses.py:103-122`, `src/gcg_sim/runner/instrument.py:20-22`, `src/gcg_sim/runner/instrument.py:98`, `docs/REPORTS.md:81`, `docs/REPORTS.md:95-97`
- **Quotes:** “"Never played" signals are descriptive”
- **What is weak:** the rule is 20+ drawn copies and a 50%+ never-played rate, with no test and no per-direction cap; "played" means only PLAY_UNIT, PLAY_BASE, PLAY_COMMAND or PAIR, so copies discarded for costs or used from hand by effects count as never played, yet the statement says the card "is often uncastable or low priority".
- **Resolution plan:** keep the existing signal; `analysis.json` splits "never played" into discarded for a cost, used by an effect from hand, still in hand at game end, and other, with a Wilson interval for each.
- **Verification** (unit): a synthetic game where a card is discarded for a cost reports it under "discarded for a cost", not "never played" in the analysis; the reference signal is unchanged.
- **Status:** resolved: `analysis.json` `never_played` splits each never-played drawn copy by fate, using re-simulation. The category wording is stated in the P6-M file. The reference signal is unchanged.
- **Option location (A30):** `gcg-eval benchmark --analysis`.
- **Evidence:** `crates/gcg-runner/tests/p6m_analysis.rs::w95_never_played_copies_are_split_by_fate` and `the_observer_partitions_the_reference_never_played_set_and_extends_seen` (12 real BO3 matches).
- **Ablation:** no AI effect (row M).

### W-96 — Benchmark-card "seen" covers only played, Burst-revealed or public cards; lift is one-sided
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `src/gcg_sim/runner/instrument.py:5-7`, `src/gcg_sim/runner/instrument.py:23-25`, `src/gcg_sim/reports/aggregate.py:184-185`, `src/gcg_sim/reports/aggregate.py:197-237`, `src/gcg_sim/reports/hypotheses.py:167-184`, `docs/REPORTS.md:80`
- **Quotes:** “Benchmark cards count as "seen" once they are played, deployed, paired, revealed as a Burst, or present in a public in-play zone.”
- **What is weak:** cards revealed from hand, discarded or seen in the trash are not counted; tokens are excluded; only positive lift (at least 0.10) becomes a hypothesis; benchmark cards get no play or draw statistics; per-card stats cover only the deck under test's main deck.
- **Resolution plan:** `analysis.json` adds a broader "observed" definition (any reveal, discard or trash entry), negative-lift reporting, and play/draw statistics for benchmark cards; the reference definitions stay unchanged.
- **Verification** (unit): a synthetic game where a benchmark card is only discarded is "observed" in the analysis and absent from the reference "seen" count.
- **Status:** resolved: `analysis.json` `benchmark_cards` adds "observed" (seen plus discards, trash, removal and plays), a lift in both directions, and the benchmark deck's own draws and plays.
- **Option location (A30):** `gcg-eval benchmark --analysis`.
- **Evidence:** `crates/gcg-runner/tests/p6m_analysis.rs::w96_a_benchmark_card_only_discarded_is_observed_not_seen`.
- **Ablation:** no AI effect (row M).

### W-97 — The results schema file name stays v1 at version 1.1.0
- **Category:** reporting · **Resolution type:** port-preserve + docs
- **Sources:** `docs/ASSUMPTIONS.md:96`, `docs/REPORTS.md:59-60`
- **Quotes:** “file name stays `results.v1.schema.json`”
- **What is weak:** the file name tracks only the major version, which can confuse readers (1.1.0 added `fast` to `ai.preset`).
- **Resolution plan:** Rust ships the same schema file and name; a schema test asserts the `$id`/version inside the file and the `schema_version` written in results agree, and the Rust report docs state the naming rule (file name = major version).
- **Verification** (unit): Rust results validate against `results.v1.schema.json` and carry 1.1.0 (or the version the lead assigns for additive changes).
- **Status:** resolved: the 1.1.0 schema keeps Python's file and name (`results.v1.schema.json`). The 1.2.0 schema is `crates/gcg-runner/schema/results.v1_2.schema.json`, and it states the naming rule in its `description`. `gcg-sim` writes 1.2.0 since commit 9ec31c0 (O9), and `gcg-eval reference-benchmark` keeps 1.1.0. The Phase 8 report docs should repeat the rule.
- **Evidence:**
  - `crates/gcg-runner/tests/p6m_reports.rs::w97_schema_files_carry_the_version_they_validate`.
  - `crates/gcg-runner/tests/results_1_2.rs::the_1_2_schema_rejects_documents_that_violate_it`.
  - Schema 1.1.0 (T-076), and 1.2.0 validation for `gcg-sim` (T-129).
- **Ablation:** no AI effect (row M).

### W-98 — Distribution statistics are integer nearest-rank values and rounded rates
- **Category:** reporting · **Resolution type:** port-preserve + Rust fix (additive analysis)
- **Sources:** `docs/REPORTS.md:77`, `src/gcg_sim/reports/stats.py:9`, `src/gcg_sim/reports/stats.py:28-29`, `src/gcg_sim/reports/stats.py:50-73`, `src/gcg_sim/reports/stats.py:75-101`, `src/gcg_sim/reports/hypotheses.py:44-66`
- **Quotes:** “`median` is the ⌈n/2⌉-th smallest value (the lower middle value when n is even)”
- **What is weak:** median, p10 and p90 are nearest-rank integers (approximate for small n); the mean and every rate and p-value are rounded to 6 digits before the Holm comparison (`_compare` stores `rnd(two_proportion_p(...))`, and `_significant` passes those values to `holm`); byte-identical `results.json` requires exactly this.
- **Resolution plan:** Rust reproduces the nearest-rank rule and Python's rounding in every byte-compared output (the `gcg-core` round/format functions already verified against Python vectors). `analysis.json` (written only with `--analysis`, W-90) adds two things. For every distribution: the unrounded mean and the interpolated p10, median and p90 (linear interpolation between order statistics, Hyndman–Fan type 7). For every hypothesis comparison: the unrounded p-value, and the Holm decision recomputed from unrounded p-values, with a flag on every hypothesis whose decision differs from the reference's.
- **Verification** (unit): vectors exported from Python (odd and even n, ties, values at rounding boundaries) give identical Rust output for the reference fields. Additive fields: for the values [1, 2, 3, 4], the reference median is 2 and the interpolated median 2.5. In a Holm family of one at α = 0.05, an unrounded p-value of 0.0500004 rounds to 0.05, so the reference keeps the hypothesis, while the unrounded decision drops it, and the analysis flags it. The reference outputs are byte-identical with and without `--analysis`.
- **Status:** resolved: `analysis.json` `distributions` gives unrounded means and Hyndman-Fan type 7 quantiles. `hypotheses` adds the unrounded p-values and their Holm decision. The reference nearest-rank values and rounding stay.
- **Option location (A30):** `gcg-eval benchmark --analysis`.
- **Evidence:** `crates/gcg-runner/tests/p6m_analysis.rs::w98_unrounded_statistics_beside_the_reference_values` and `played_events_feed_the_own_turn_distribution`.
- **Ablation:** no AI effect (row M).

### W-99 — Decision logs make outputs large; reports and logs truncate
- **Category:** reporting · **Resolution type:** Rust fix (opt-in compression)
- **Sources:** `docs/REPORTS.md:19`, `src/gcg_sim/ai/base.py:15`, `src/gcg_sim/ai/base.py:84`, `src/gcg_sim/reports/hypotheses.py:23`, `src/gcg_sim/reports/hypotheses.py:128`, `src/gcg_sim/reports/hypotheses.py:205`, `src/gcg_sim/reports/results.py:27`, `src/gcg_sim/reports/results.py:90`, `src/gcg_sim/reports/summary.py:8`
- **Quotes:** “This makes them large.”; “LOG_ALTERNATIVES = 5”
- **What is weak:** `--decision-log` writes per-decision search statistics into `games.ndjson` and replays; the decision log keeps the top 5 alternatives, hypotheses keep 3 per direction, draw replays are capped at 1 and the summary shows 10 benchmark cards.
- **Resolution plan:** keep formats and limits byte-identical by default; add an opt-in `--compress` that writes `games.ndjson.zst` (decompresses to the identical bytes); full search detail goes to the gcg-eval trace (W-21) rather than raising the limits.
- **Verification** (unit + e2e): decompressed output is byte-identical to the uncompressed run; default output is unchanged.
- **Status:** resolved: `--compress` replaces `games.ndjson` by `games.ndjson.zst` (the approved `zstd` encoder, feature `zstd`) or by `games.ndjson.gz` (the in-house safe-Rust gzip writer, `crates/gcg-runner/src/reports/gzip.rs`). Defaults and limits are unchanged.
- **Option location (A30):** `gcg-eval benchmark --compress`.
- **Evidence:**
  - `crates/gcg-runner/tests/p6m_reports.rs::w99_gzip_output_decompresses_to_the_same_bytes`: `gzip -dc` gives back the exact input.
  - `crates/gcg-eval/tests/p6m_cli.rs::opt_ins_add_files_and_keep_the_reports`.
  - With `--features zstd`: `w99_zstd_output_decompresses_to_the_same_bytes`.
- **Ablation:** no AI effect (row M).

### W-100 — The summary's reproduce command omits the agent factory and can name a deck that is not a path
- **Category:** reporting · **Resolution type:** Rust fix
- **Sources:** `src/gcg_sim/reports/writer.py:44-51`, `src/gcg_sim/cli.py:80`, `tests/bench/__pycache__/test_reproduce_line.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “decks = [d.source or d.name for d in (c.deck_under_test, c.benchmark_deck)]”; “summary.md's reproduce command carries PYTHONPATH and the agent factory of the run”
- **What is weak:** a run with `GCG_SIM_AGENT_FACTORY` (recorded under `ai.factory`) reproduces with the default AI; an in-memory deck without a source falls back to its name; the lost later branch fixed this.
- **Resolution plan:** the Rust reproduce line names the exact AI identity, and when a deck has no source file, it writes the deck to the report directory and points the command at it. Under W-108 the release CLI reads no agent-factory variable, so the AI identities a release command can reproduce are its presets (`--ai-preset`). A run whose agent was injected through the library API or the test-only build feature cannot be reproduced by a release command. For such a run, the reproduce line says so and names the injected agent recorded under `ai.factory`, instead of printing a command that would silently use the default AI. This changes `summary.md`, so the equivalence normaliser (`scripts/normalize_outputs.py`) must treat the reproduce line as an implementation field; the lead decides whether that is acceptable or the fix stays behind a flag.
- **Verification** (e2e): running the printed command from a fresh shell reproduces `results.json` byte for byte for a file deck, an in-memory deck and each non-default preset (`fast`, `strong`). For a run with an injected test agent, the reproduce line carries the "not reproducible with the release CLI" marker and names the agent.
- **Status:** resolved (A23): results 1.2.0 runs write the corrected reproduce line. Decks are copied into the report directory, and an injected factory gets the "not reproducible with the release CLI" marker. `gcg-sim benchmark` writes 1.2.0 since commit 9ec31c0 (O9), so it prints the corrected line. `gcg-eval reference-benchmark` (1.1.0) keeps Python's line byte for byte.
- **Evidence:**
  - `crates/gcg-runner/tests/p6m_reports.rs::w100_the_corrected_reproduce_line_of_results_1_2`.
  - `crates/gcg-cli/tests/benchmark.rs::needs_engine_benchmark_writes_reports_and_prints_a_summary`: the reproduce line names the deck files and the preset, and re-runs to the same `results.json`.
  - `crates/gcg-runner/tests/results_1_2.rs`.
- **Ablation:** no AI effect (row M). The line is report text.

### W-101 — conflict_resolutions lists only resolutions whose id names a deck card
- **Category:** reporting · **Resolution type:** Rust fix (additive field)
- **Sources:** `src/gcg_sim/reports/versions.py:93-99`, `src/gcg_sim/reports/versions.py:116-135`, `docs/REPORTS.md:82`
- **Quotes:** “numbers.update(cid.split(":")[1:])”
- **What is weak:** card numbers are inferred by splitting conflict ids plus curated lists; rule-level interpretations (e.g. `most_defensible_reading` on keywords or timing) that change how a deck's cards play are not listed, so the report understates which interpretations affected a run.
- **Resolution plan:** keep the existing field; `analysis.json` adds `interpretations_used`: every conflict or interpretation id whose program, rule or keyword was actually exercised in the run's games (recorded by the engine when an interpretation-tagged code path runs, per W-56).
- **Verification** (e2e): a run with a <Suppression> deck lists the Suppression-with-Base interpretation; the existing field is unchanged.
- **Status:** resolved: `analysis.json` `interpretations_used` lists every interpretation of `docs/port/audit/interpretations.json` (W-56) whose tagged cards were played, deployed, paired, activated, attacked with or revealed as a Burst. The existing `conflict_resolutions` field is unchanged. The engine records when an anchored interpretation's code path runs (`gcg_engine::diag::interpretation_runs`, read-only and thread-local, in no compared output), and `interpretations_used` lists an interpretation by that record too (`docs/port/eval/weakness-status/GAP-B.md`). 14 of the 27 interpretations are anchored, 12 of them among the 14 rule-level ones: I-01, I-02, I-03, I-04, I-05, I-06, I-10, I-11, I-13, I-14, I-15, I-16, I-17 and I-18. The last four anchors:
  - I-04 at an `Apply` on an `All` selection, whose lasting effect holds the cards present when it resolves (`crates/gcg-engine/src/interp/lasting.rs:76-78`);
  - I-11 where the playability walk reaches a mode selection (`crates/gcg-engine/src/interp/choose.rs:79`) and at the mode choice (`crates/gcg-engine/src/interp/mod.rs:696`);
  - I-13 where a card becomes its Unit variant (`crates/gcg-engine/src/interp/deploy.rs:139`) and where a variant leaves the battle area (`crates/gcg-engine/src/core.rs:220-226`);
  - I-17 where a linked pairing emits LINKED, its only emitter (`crates/gcg-engine/src/interp/play.rs:92-93`).

  The other two rule-level interpretations are compile-time readings of the card text with no engine code path, and `not_traceable_from_games` lists exactly these two. I-21: the compiler moves the sentences after a leading "If"/"If you do", "Then" parts included, into that condition's block (`src/gcg_sim/effects/compiler/grammar.py:1641-1645`, `:1660-1680`). The block lowers to one `JumpIfNot` or `JumpIfNotDid` (`src/gcg_sim/effects/program.py:116-137`), which the engine runs for every condition (`crates/gcg-engine/src/interp/mod.rs:602-621`). I-24: the compiler routes a Pilot's text (`src/gcg_sim/effects/compiler/__init__.py:32-50`). The card-level interpretations keep the card trace, which over-approximates: a listed interpretation's code path may not have run.
- **Option location (A30):** `gcg-eval benchmark --analysis`.
- **Evidence:**
  - `crates/gcg-runner/tests/p6m_analysis.rs::w101_a_suppression_card_lists_the_suppression_interpretation` (I-18 listed by its card; `conflict_resolutions` untouched; `not_traceable_from_games` is I-21 and I-24).
  - `p6m_analysis.rs::w101_a_rule_level_interpretation_the_engine_records_is_listed` (pass): 4 random BO1 games re-simulated, I-05 and I-15 (rule-level, without cards) listed with `engine_games` = 4 and `card_games` = 0.
  - `crates/gcg-engine/tests/gapb_interpretations.rs` (pass): 300 random games, each played twice with identical records and states (I-17 in 253 of them). The scenarios of the Python tests record each new anchor: `w101_a_lasting_all_your_units_effect_records_i04` (FAQ Q105), `w101_a_modal_command_records_i11` (ST14-013:Q469 and the mode choice), `w101_a_card_deployed_as_a_unit_records_i13` (GD05-089:Q389, the deploy and the move to the trash) and `w101_when_linked_on_a_linked_pairing_records_i17`. The controls record nothing: a Command without modes no I-11, a chosen target no I-04, a Burst added to the hand no I-13 (`w101_a_burst_added_to_the_hand_does_not_record_i13`), a pairing without a link no I-17.
  - Tier-1 corpus 3804/3804 with the anchors.
- **Ablation:** no AI effect (row M).

### W-102 — The report directory is not updated atomically as a set
- **Category:** reporting · **Resolution type:** Rust fix
- **Sources:** `src/gcg_sim/reports/writer.py:1-2`, `src/gcg_sim/reports/writer.py:79-91`, `src/gcg_sim/reports/writer.py:94-110`, `docs/REPORTS.md:26-28`
- **Quotes:** “so an interrupted write never leaves a corrupt file”
- **What is weak:** each file is written atomically, but stale replays are deleted and new replays, `games.ndjson`, `summary.md` and `timing.json` are written before `results.json`; an interruption after them leaves new files next to the previous run's `results.json`.
- **Resolution plan:** Rust writes the whole set into a sibling temporary directory and swaps it in with one rename (keeping the previous set until the swap), so readers see the old set or the new one, never a mix. File contents are unchanged.
- **Verification** (unit): a test that aborts the writer after each file leaves either the complete old set or the complete new set.
- **Status:** resolved: `reports::write_report_set` builds every file in memory, writes them into a staging directory and swaps it in. A directory holding foreign files is updated in place, file by file, after all are written. `gcg-eval benchmark` uses it, and so does `gcg-sim benchmark`: `cli_support::benchmark_command` calls `write_report_set` (`crates/gcg-runner/src/cli_support.rs`, switched at integration, commit 8af9b0e). The contents equal `write_reports_with`'s byte for byte.
- **Evidence:** `crates/gcg-runner/tests/p6m_reports.rs::w102_an_abort_after_any_file_keeps_the_complete_old_set` (an abort after each file of a BO3 set leaves the old set byte for byte, in both modes) and `w102_a_symlinked_report_directory_stays_a_link`.
- **Ablation:** no AI effect (row M).

### W-103 — code_digest hashes every file of the installed package; the version can be "unknown"
- **Category:** reporting · **Resolution type:** Rust fix
- **Sources:** `src/gcg_sim/reports/versions.py:25-29`, `src/gcg_sim/reports/versions.py:32-52`
- **Quotes:** “return "unknown"”
- **What is weak:** in an editable or source checkout any stray non-`.pyc` file under `src/gcg_sim` changes the digest, so results look non-comparable; `package_version()` returns "unknown" when the package is not installed.
- **Resolution plan:** the Rust `code_digest` is computed at build time from the crate sources and embedded data listed in a manifest (no stray files), and the package version comes from the build; `scripts/normalize_outputs.py` already treats both fields as implementation-specific.
- **Verification** (unit): adding an unrelated file to the source tree does not change the digest; changing an engine source file does.
- **Status:** resolved by construction (Phase 3): `code_digest` is computed at build time over `build-support/digest-manifest.txt`, by the `build.rs` of `gcg-cli` and `gcg-eval`. The version is `CARGO_PKG_VERSION`. The manifest also lists `src/gcg_sim/data/gcgapi/products.json`, which `gcg-eval validate` embeds (W-76).
- **Evidence:** `crates/gcg-cli/tests/code_digest.rs`: an unrelated file leaves the digest unchanged, and an edited source file changes it.
- **Ablation:** no AI effect (row M).

### W-104 — timing.json is wall-clock based and the only non-reproducible report
- **Category:** reporting · **Resolution type:** Rust fix (additive fields)
- **Sources:** `src/gcg_sim/reports/writer.py:60-76`, `docs/REPORTS.md:55`
- **Quotes:** “wall-clock measurements; the only non-reproducible report file”
- **What is weak:** `games_per_second` is games over wall seconds including worker start-up; no CPU-time measure is reported.
- **Resolution plan:** Rust `timing.json` keeps the wall-clock fields and adds per-worker CPU time, start-up time and a node/decision count (deterministic) so throughput can be compared across machines; timing stays out of every deterministic output.
- **Verification** (unit): fields present; decisions per game equal the count in `games.ndjson`; the equivalence comparison drops `timing.json` as now.
- **Status:** resolved, by the decision recorded in `docs/port/eval/weakness-status/GAP-C.md` (the lead may override it): `gcg-eval benchmark`'s `timing.json` adds the deterministic `decisions` and `decisions_per_game` as its machine-independent work measure, with no CPU-time or start-up-time field. `gcg-sim`'s `timing.json` keeps Python's keys plus `topology`. The reasons:
  - Safe std has no CPU-time clock. `getrusage` and `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` need `unsafe` FFI, which the lints deny outside `crates/gcg-core/src/libm.rs`, and no safe wrapper crate is in A8's list or the cargo cache.
  - A per-worker busy-seconds or start-up measure would be wall time again, and would add another non-reproducible number to the one non-reproducible report.
- **Option location (A30):** the added fields appear in `gcg-eval benchmark`'s `timing.json`.
- **Evidence:**
  - `crates/gcg-eval/tests/p6m_cli.rs::t153_benchmark_writes_gcg_sims_bytes_and_ignores_the_factory_variable`: `decisions_per_game` equals the action counts of `games.ndjson`, and T-153 and the equivalence comparison ignore `timing.json`.
  - `crates/gcg-eval/tests/gapc_tools.rs::w75_w76_w104_analysis_scope_block_and_timing_fields` (pass) pins the keys of `gcg-eval benchmark`'s `timing.json` (Python's keys, `topology`, `decisions`, `decisions_per_game`) and checks `decisions_per_game` against `games.ndjson`.
- **Ablation:** no AI effect (row M).

## Runner

### W-105 — Interrupted or failed runs discard all work: no checkpoint or resume
- **Category:** runner · **Resolution type:** Rust fix
- **Sources:** `docs/REPORTS.md:26-28`, `src/gcg_sim/runner/run.py:1-6`, `src/gcg_sim/runner/run.py:97-106`, `src/gcg_sim/runner/game.py:129-138`
- **Quotes:** “**Ctrl-C** stops the worker processes, writes no reports, and exits with 130.”; “so no partial results are returned”
- **What is weak:** a single game failure aborts the whole benchmark and Ctrl-C writes nothing, so a long run loses all finished matches.
- **Resolution plan:** the Rust runner appends each finished match record to a checkpoint file in the output directory (match index, seeds, record, config digest); `--resume` reloads it, verifies the config digest and plays only the missing matches. Because every match is a pure function of its seed, the final reports are byte-identical to an uninterrupted run. Failure semantics (exit code, no reports) stay the same unless `--resume` is used.
- **Verification** (e2e): kill a run after k matches, resume, and compare all reports byte for byte with an uninterrupted run; a resume with a different config is refused.
- **Status:** resolved: `--resume` writes `checkpoint.ndjson`: a configuration header with its digest, then one line per finished match. A resume verifies the digest, drops a cut-off final line and plays the missing matches with the same `play_match`. Without `--resume` nothing changes.
- **Option location (A30):** `gcg-eval benchmark --resume`.
- **Evidence:**
  - `crates/gcg-runner/tests/p6m_reports.rs::w105_a_resumed_run_writes_the_uninterrupted_reports`: at 1 and 3 workers the reports equal an uninterrupted run's byte for byte, except `timing.json`.
  - `crates/gcg-eval/tests/p6m_cli.rs::w105_an_interrupted_resume_run_finishes_with_the_uninterrupted_reports`: SIGINT gives exit 130 with the checkpoint kept, and another seed is refused.
- **Ablation:** no AI effect (row M).

### W-106 — Worker termination: waitpid race, a private executor attribute, AI tools without interrupt handling
- **Category:** runner · **Resolution type:** Rust fix (by construction)
- **Sources:** `docs/PROGRESS.md:93-96`, `src/gcg_sim/runner/run.py:26`, `src/gcg_sim/runner/run.py:56-64`, `src/gcg_sim/ai/selfplay.py:113-118`, `src/gcg_sim/ai/tuning.py:165-169`, `src/gcg_sim/ai/tuning.py:292-296`, `d9d41af`
- **Quotes:** “the caller and the executor's manager thread both joined the terminated workers and raced for `waitpid`”; “executor._executor_manager_thread”; “joining them from the caller raced it for waitpid and left a reaped worker reported alive (test_interrupt_terminates_workers_and_returns_nothing failed 5/80 on main, 0/80 now)”; “with ProcessPoolExecutor(max_workers=workers, mp_context=get_context("spawn")) as pool:”
- **What is weak:** the interrupt test failed 5/80 before the fix; the fix relies on a private `ProcessPoolExecutor` attribute and a 10 s join timeout; `compare`, `throughput` and `tuning` use a bare pool without interrupt handling.
- **Resolution plan:** Rust runs games on threads (no worker processes, no `waitpid`) with a shared cancellation flag checked between decisions; results are collected in index order so worker count never changes output. gcg-eval and the tuning tool use the same runner.
- **Verification** (unit + e2e): an interrupt test run 200 times returns promptly with no reports every time; results identical for 1, 4 and 10 workers.
- **Status:** resolved by construction (Phase 3): games run on `gcg_core::pool` threads, with the shared `Cancel` checked between decisions. Results are collected in match order.
- **Evidence:**
  - `crates/gcg-runner/tests/p6m_interrupt.rs::w106_two_hundred_interrupts_return_no_results`: 200 real SIGINTs at 1 and 4 workers.
  - `crates/gcg-cli/tests/interrupt.rs` and `sigint.rs`: exit 130 and no reports (T-075 in the register).
  - `run.rs::results_do_not_depend_on_workers` (T-074).
- **Ablation:** no AI effect (row M).

### W-107 — Spawn start-method constraints on the Python API
- **Category:** runner · **Resolution type:** Rust fix (not applicable by design)
- **Sources:** `docs/REPORTS.md:139`, `docs/REPORTS.md:154-156`
- **Quotes:** “required: workers are started with the "spawn" method”
- **What is weak:** the Python API needs a `__main__` guard and a picklable `agent_factory` when workers > 1.
- **Resolution plan:** the Rust runner uses threads, so neither constraint exists; agent factories are trait objects that are `Send + Sync`, checked at compile time. If Python bindings are offered, they release the GIL and run games in Rust threads.
- **Verification** (unit): a custom agent factory works with 10 workers in a test binary without any guard; a non-`Send` factory fails to compile (compile-fail test).
- **Status:** resolved by construction (Phase 3): agent factories are `AgentFactory: Sync` trait objects shared by the worker threads. No start-method constraint exists.
- **Evidence:**
  - `crates/gcg-runner/tests/p6m_reports.rs::w107_a_custom_factory_runs_on_ten_workers`.
  - The compile-fail doctest in `crates/gcg-runner/src/checkpoint.rs`: a factory holding a `Cell` does not compile.
  - `contracts.rs::factories_and_records_cross_threads`.
- **Ablation:** no AI effect (row M).

### W-108 — The GCG_SIM_AGENT_FACTORY test hook can replace the AI in a benchmark
- **Category:** runner · **Resolution type:** Rust fix
- **Sources:** `docs/REPORTS.md:149-156`
- **Quotes:** “Do not set it for real benchmarks.”
- **What is weak:** an environment variable swaps the search AI for any callable; it is recorded under `ai.factory` but easy to leave set.
- **Resolution plan:** the Rust CLI does not read an agent-factory environment variable; test agents are injected only through the library API or a test-only build feature, and `ai.factory` stays in results for equivalence.
- **Verification** (unit + e2e): the release CLI ignores the variable (results equal a run without it). The test feature's injected agent is recorded under `ai.factory`, and the summary's reproduce line marks that run as not reproducible with the release CLI (W-100).
- **Status:** resolved: no binary reads an agent-factory variable. Test agents are reachable only through `gcg-eval reference-benchmark --agent` and the library API. They are recorded under `ai.factory`, and a 1.2.0 run with one gets the "not reproducible" reproduce line (W-100).
- **Evidence:**
  - `crates/gcg-runner/tests/p6m_reports.rs::w108_no_crate_reads_an_agent_factory_variable`.
  - `crates/gcg-eval/tests/p6m_cli.rs::t153_...`: with `GCG_SIM_AGENT_FACTORY` set, the run writes the same bytes as without it.
  - T-073: `reference-benchmark --agent test-random` equals Python with the hook.
- **Ablation:** no AI effect (row M).

### W-109 — Simultaneous defeat and the chooser after a drawn game rest on unconfirmed suggestions
- **Category:** runner · **Resolution type:** port-preserve + reporting
- **Sources:** `docs/ASSUMPTIONS.md:55`, `docs/REPORTS.md:38-42`, `docs/research/OFFICIAL_SOURCES.md:180-187`, `docs/research/OFFICIAL_SOURCES.md:260-261`, `docs/research/RULES_INDEX.md:209`, `src/gcg_sim/data/official/bo3_match_rules.json:2-12`
- **Quotes:** “In BO3 match scoring the turn player at the end loses (TRM 5.2 top-cut rule), so every match resolves.”; “No rule covers who chooses Player One after a drawn game.”; “Lead to confirm.”
- **What is weak:** a 1v1 double defeat is undefined by the rules (11-2-1 vs 1-2-1) and is a game draw in statistics; in BO3 scoring the turn player loses (TRM 5.2 is the top-cut rule but is applied to every BO3; in Swiss a drawn match scores 1 point). After a drawn game the game-1 random procedure picks the chooser. The packaged data file still carries "Lead to confirm." suggestion text.
- **Resolution plan:** Rust implements both exactly; every report states the number of double defeats and how they were scored, and an opt-in `--swiss-draws` scoring mode reports matches with a double-defeat decider as drawn (1 point) in `analysis.json` only. The data file's wording is a packaged source and stays unchanged; rewording it is a data-workflow task.
- **Verification** (unit + e2e): a scripted double defeat is a game draw and a BO3 loss for the turn player (equal to Python); the report counts it; `--swiss-draws` shows the alternative score only in the analysis.
- **Status:** resolved: the engine and runner implement both rules exactly (`game.rs::match_point_of`), and `results.json` already states the double defeats. `analysis.json` `double_defeats` repeats them. `--swiss-draws` adds the Swiss scoring in `analysis.json` only. The packaged data wording stays.
- **Option location (A30):** `gcg-eval benchmark --analysis --swiss-draws`.
- **Evidence:**
  - `game.rs::tests::match_point_rules`.
  - `crates/gcg-runner/tests/p6m_analysis.rs::w109_a_double_defeat_decider_is_a_swiss_draw_in_the_analysis_only`.
  - `crates/gcg-eval/tests/p6m_cli.rs::opt_ins_add_files_and_keep_the_reports`: `--swiss-draws` without `--analysis` exits 2.
- **Ablation:** no AI effect (row M).

## Tooling

### W-110 — Robustness fuzzing uses only random agents
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/SPEC.md:57`, `docs/PROGRESS.md:30-31`, `docs/status.json`, `src/gcg_sim/tools/fuzz.py:1-6`, `src/gcg_sim/tools/fuzz.py:46-73`, `src/gcg_sim/tools/fuzz.py:107-110`
- **Quotes:** “≥10,000 random-agent games, zero crashes, zero invariant violations”; “plays games with uniformly random legal actions”
- **What is weak:** invariants are checked after every action in random games with one- or two-colour generated decks; AI-driven lines are checked only through the derived-view cache guard in random and greedy games; turn-limit games count as failures.
- **Resolution plan:** Rust fuzz adds strategic agents (greedy and strengthened `fast` with a small node budget) and mixed-agent games over the same generated decks, with all invariants and the paranoid checks (W-49) on; seeds and decks are derived as in Python.
- **Verification** (e2e): 10,000 random and 1,000 strategic fuzz games with zero crashes and zero invariant violations; the random-agent fuzz results equal Python's on the shared seeds.
- **Status:** resolved: `gcg-diff fuzz` has the strategic agents `greedy`, `search:<preset>`, `ref:<preset>` and `mixed:A,B`, with invariants after every action. The random-agent half passes (T-055). The 1,000 strategic games of PLAN §9.4 ran in the paranoid build with the shipped M15 presets, with 0 failures: 400 `greedy`, 400 `search:fast` and 200 `mixed:greedy,search:fast` (`docs/port/eval/weakness-status/GAP-D.md`). Game 221 of the `search:fast` run, which reached the W-143 redirect cycle in Phase 4 and failed T-056 then, completes: `engine_error_leaf` scores that line inside the search, and it is in both shipped presets. The runs used the M15 `fast`, before M15b added two mechanisms to it; no strategic fuzz run of the M15b presets is recorded. `docs/port/tests.json` still records T-056 as failing (a lead file).
- **Evidence:**
  - Random agent: `gcg-diff fuzz --games 10000 --seed 2026 --workers auto --compare port/corpus/v1/tier1/fuzz/summary-2026-10000.json`. 10,000 games, 856,034 actions, 0 failures, and the summary and every per-game row equal Python's (T-055).
  - Strategic paranoid runs with the shipped M15 presets (seed 2026, `docs/port/eval/GAP-D/w110-strategic-fuzz.txt`, the gate's output verbatim, on a clean clone at 3f4aafb): `greedy` 400 games, 31,936 actions, 0 failures; `search:fast` 400, 29,781, 0; `mixed:greedy,search:fast` 200, 14,650, 0. Every game ended by battle damage. The official GAP-D gate run at 54c2bd2 reproduced the three action counts exactly.
  - Commit-seed fuzz in the same gate: 2,000 games at seed 1585305945398772890 (commit 54c2bd2), 170,257 actions, 0 failures (T-056, first part).
  - Earlier strategic runs (seed 2026, `docs/port/eval/P6-N/w110-strategic-fuzz.txt`, pre-M15 `fast`):

    | Agent | Build | Games | Actions | Failures |
    | --- | --- | ---: | ---: | --- |
    | `greedy` | paranoid | 400 | 31,936 | 0 |
    | `search:fast` | release | 400 | 27,490 | 0 (game 221 completes) |
    | `mixed:greedy,search:fast` | release | 200 | 14,558 | 0 |
    | `search:fast` | paranoid | 400 | not completed | — |
    | `mixed:greedy,search:fast` | paranoid | 200 | not completed | — |

  - Earlier commit-seed fuzz: 2,000 games, 172,794 actions, 0 failures.
  - `crates/gcg-search/tests/engine_error_leaf.rs` passes.
- **Ablation:** no AI effect (row N): gate-tool runs, with no source change.

### W-111 — Coverage gates count tags, not behaviour
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `src/gcg_sim/tools/marks.py:1-5`, `tests/effects/test_card_coverage.py:33-40`
- **Quotes:** “Coverage gates and the rules traceability report are computed from the tags in the source,”
- **What is weak:** a card, rule, FAQ entry or ruling counts as covered when any test carries its tag; nothing checks what the test asserts.
- **Resolution plan:** Rust coverage combines tags with execution evidence: a tagged test counts only if, when run, it executes the tagged card's program (or the tagged rule's code path, via the interpretation ids of W-56), recorded by a test-only coverage hook.
- **Verification** (unit): a tagged test that never touches its card is reported as uncovered. The bar is 100% coverage with execution evidence (every card, testable rule, FAQ entry and ruling has a tagged Rust test that executes it, or a written N/A reason). Execution evidence can only lower the count below what tags alone give, so "the same coverage as the Python tags" is not a valid bar. The coverage report lists every Python test that is tagged but never executes its card or rule. Each such gap gets a ported test that does execute it, or is reported to the lead as a Python-side test gap.
- **Status:** resolved: for rules, FAQ entries and rulings, tag-based coverage stands by the lead's decision (`docs/port/PLAN.md` §0.1, end of Phase 6; `docs/port/eval/weakness-status/GAP-B.md`), the second option below. Done for cards: `gcg-diff coverage --check` counts a passing tagged trace as execution evidence for a card only when the test's executed-program record shows the card's program ran (`scripts/test_census.py`, `docs/port/eval/P6-N/test-execution.json`). Rules, FAQ entries and rulings stay tag-based (a passing trace or native test per id). The verification's bar of 100% execution evidence for them needs per-rule code-path ids in the engine. The two options:
  - probes: extend the engine's interpretation counters (`gcg_engine::diag::Interpretation`, W-101) rule by rule, about 395 testable rules over about 14,600 lines of `gcg-engine/src`, and read them in `crates/gcg-diff/src/coverage.rs` while the per-test traces replay (L effort);
  - a recorded decision that tag-based coverage stands for them (taken). GAP-B recommends this: every tagged Python test is replayed decision by decision against Python's trace (3213/3213 per-test traces, 311/311 census games), so a passing tagged test reproduces Python's behaviour exactly, and a test that tags a rule it never exercises is a Python test-quality gap.
- **Evidence:**
  - `gcg-diff coverage --check` passes. Of 840 non-vanilla cards, 724 are run by a passing tagged trace; 1 runs only in a census game (GD04-089, a Python-side test gap); 115 have no program; 0 have a program and no execution evidence.
  - 545 of 2,270 card-tagged Python tests never run their tagged cards' programs. Each is listed in `test-execution.json`.
  - Unit test `coverage::tests::w111_a_tagged_test_that_never_runs_its_card_is_not_execution_evidence`.
- **Ablation:** no AI effect (row N).

### W-112 — The long slow suite is not deselected by default
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `pyproject.toml:53-62`, `scripts/verify_all.sh:23`, `tests/test_no_placeholders.py:10`, `tests/slow/test_robustness.py:15-24`, `tests/slow/test_ai_strength.py:43-56`, `tests/runner/test_replay.py:43-49`, `tests/ai/test_throughput.py:12`, `tests/ai/test_throughput.py:20`, `tests/ai/test_throughput.py:26`
- **Quotes:** “addopts = "-ra --strict-markers --import-mode=importlib"”; “@pytest.mark.slow”; “assert lower > 0.50”; “def test_every_example_pairing_replays(”
- **What is weak:** `addopts` has no `-m` filter (verify_all runs `-m ""`), so the default run includes the 10,000-game robustness test, the 400-game strength tests, the replay of every example pairing (6 permutations x 60 BO3 matches) and the throughput tests; there are no skip or xfail markers (which is good) and `src/` bans TODO/FIXME placeholders.
- **Resolution plan:** Rust tests are tiered: `cargo test` runs unit and fast tests; slow tests are `#[ignore]`d with a reason and run by `cargo test -- --ignored` in the full gate (never skipped there). No test is weakened or xfailed.
- **Verification** (e2e): the full gate script runs both tiers and fails if any ignored test fails; a meta-test asserts every ignored test belongs to the slow tier list.
- **Status:** resolved: slow tests are gated by their package's `slow` feature, never by `#[ignore]` (A12). `scripts/slow-tier.txt` lists the slow tier. The gate runs the fast tier and then the slow tier. In `--full` it runs the tier-2 tests after the tier-2 corpus is written.
- **Evidence:** `crates/gcg-diff/tests/p6n_tooling.rs::w112_the_slow_tier_is_the_listed_one_and_the_gate_runs_both_tiers` and `w112_slow_gating_is_recognised_on_files_and_functions`.
- **Ablation:** no AI effect (row N).

### W-113 — Test helpers stop silently at step caps
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `src/gcg_sim/testkit.py:348-360`, `src/gcg_sim/testkit.py:363-380`, `src/gcg_sim/testkit.py:419-428`
- **Quotes:** “def pass_all(st: GameState, *, max_steps: int = 50) -> None:”; “for _ in range(200):”
- **What is weak:** `pass_all` stops after 50 steps, `to_next_turn` after 200 steps and `run_random` after 5,000 actions, all without an assertion. A test can therefore pass without reaching the state it intended. `to_next_turn` fails loudly only on an unexpected decision kind, not when it runs out of steps.
- **Resolution plan:** the Rust testkit helpers return an error (or panic in tests) when they hit their cap, with an explicit `allow_cap` variant for the rare test that wants it.
- **Verification** (unit): a helper forced past its cap fails the test; every ported test passes without `allow_cap` unless documented.
- **Status:** resolved: `pass_all`, `to_next_turn` and `run_random` of the Rust testkit fail with `TestkitError::Assertion` at their cap. The `*_allow_cap` variants return `CapOutcome::Capped` instead.
- **Evidence:** unit tests `testkit::tests::w113_pass_all_fails_at_its_cap_and_the_allow_cap_variant_stops`, `w113_to_next_turn_fails_at_its_cap_and_reaches_main_within_it` and `w113_run_random_fails_when_the_game_is_not_over_at_its_cap`. Every ported test passes without `allow_cap`.
- **Ablation:** no AI effect (row N). The testkit is in no shipped binary.

### W-114 — The documented-command checker: shell blocks only, a network-dependent skip, timeouts that leave children running
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/PROGRESS.md:52-53`, `docs/research/DATA_INGESTION.md:7`, `docs/status.json:15`, `docs/REPORTS.md:133-147`, `scripts/check_doc_commands.py:1-8`, `scripts/check_doc_commands.py:22-32`, `scripts/check_doc_commands.py:86-99`, `tests/bench/__pycache__/test_check_doc_commands.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “1 skipped with a reason: it needs a network clone”; “doc-check: skip needs a full git clone of gcg-api at f57b7c0 (network)”; “(?P<lang>bash|sh|shell)”; “runs each documentation block in its own process group and”
- **What is weak:** only bash/sh/shell fences run (the REPORTS Python API example never does); `.claude/skills/gcg-refresh-data` docs are not in the default files; the `ingest_profile.py` full-clone block is skipped; `subprocess.run(timeout=…)` kills only the bash child, so worker pools started by a block outlive it (the fix exists only in lost bytecode).
- **Resolution plan:** the Rust-side doc checker (used for the port's own docs) runs shell and Rust example blocks, puts each block in its own process group and kills the group on timeout or exit, includes the skill docs, and marks network blocks as "network" so a network-enabled CI job runs them instead of skipping them forever.
- **Verification** (unit): a doc block that starts a background `sleep 600` is killed with its group at the timeout; a Rust example block compiles and runs; the network block runs in the network job.
- **Status:** resolved: `scripts/check_doc_commands.py` runs each block in its own process group and kills the group at exit and at the timeout. `doc-check: network` blocks run only with `--network`. `doc-check: rust` blocks compile and run as programs. Follow-up for the lead: mark the refresh skill's blocks and add `.claude/skills/gcg-refresh-data/**/*.md` to the default set (P6-N lead note 3).
- **Evidence:** `python3 scripts/check_doc_commands.py --selftest`: a `sleep 600 &` block leaves no process; a block waiting on `sleep 600` with a 2 s timeout is stopped with its group; a Rust example compiles and runs; a network block is listed offline and runs with `--network`.
- **Ablation:** no AI effect (row N).

### W-115 — Research artifacts are pinned to an old snapshot and outside the gate; regeneration is manual
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/ASSUMPTIONS.md:43-46`, `.claude/skills/gcg-refresh-data/SKILL.md:287-291`, `.claude/skills/gcg-refresh-data/references/file-map.md:20-21`, `scripts/ingest_profile.py:31`, `6078099`
- **Quotes:** “`docs/research/data_profile.json` and `DATA_INGESTION.md` stay a profile of `f57b7c0`”; “Bump cluster_effects.py's GCG_API_COMMIT and regenerate effect_templates.json, work_packages.json and EFFECT_TEMPLATES.md”
- **What is weak:** `ingest_profile.py` (`PINNED_COMMIT`) and `cluster_effects.py` (`GCG_API_COMMIT`) pin commits by hand and are not part of the verification gate, so they drift silently (see W-128).
- **Resolution plan:** the port treats research artifacts as Python-side documents: Rust reads none of them. A Rust data-gate check compares the snapshot commit stated in each research artifact with the packaged lock and prints a "stale research artifact" warning in the gate summary (non-failing, since regenerating them is a Python-side task handed to the lead).
- **Verification** (unit): with the current files the check reports `DATA_INGESTION.md` and `data_profile.json` as profiles of f57b7c0 against the packaged 39c85b5.
- **Status:** resolved: `scripts/gate_summary.py`, run by the gate, prints a non-failing `stale research artifact` warning for each artefact whose data commit differs from the packaged one. Regenerating the artefacts is Python-side work (P6-N lead note 6).
- **Evidence:**
  - `python3 scripts/gate_summary.py` reports DATA_INGESTION.md and data_profile.json as profiles of f57b7c0 against the packaged 39c85b5.
  - `crates/gcg-diff/tests/p6n_tooling.rs::w115_w153_the_gate_summary_flags_stale_research_and_shows_the_news_scan_date` and `w115_w153_negative_controls_follow_the_packaged_commit_and_the_news_dates`.
- **Ablation:** no AI effect (row N).

### W-116 — Research scripts disagree with the engine on canonical printing and reminder-text stripping
- **Category:** tooling · **Resolution type:** port-preserve + test
- **Sources:** `scripts/cluster_effects.py:76-82`, `scripts/cluster_effects.py:108-112`, `scripts/cluster_effects.py:540-556`, `docs/research/EFFECT_TEMPLATES.md:82-84`, `docs/research/EFFECT_TEMPLATES.md:942`, `docs/research/DATA_INGESTION.md:46-48`, `docs/research/DATA_INGESTION.md:123-133`, `src/gcg_sim/cards/db.py:100-109`, `src/gcg_sim/data/overrides.json:63`, `src/gcg_sim/data/overrides.json:69`, `src/gcg_sim/data/overrides.json:228-231`
- **Quotes:** “R-001 -> R-001_p4”; “its content ends with '.' or has 5+ words”
- **What is weak:** `cluster_effects` takes the lowest `_pN` (Edition Beta, including the defective R-001_p4), `ingest_profile` the lowest non-Beta, the engine an override-pinned printing (R-001_p7) with a fallback that sorts `product_id` lexicographically (`_p10` < `_p2`); the scripts use two different reminder-stripping heuristics and the engine compiler a third. Harmless today only because these cards are vanilla.
- **Resolution plan:** Rust implements only the engine's rule (overrides, then the same fallback, reproduced exactly) and a test asserts that for every card number the fallback is either unused or agrees with the override, so the lexicographic quirk can never decide a non-vanilla card.
- **Verification** (unit): for every card number Rust and Python pick the same canonical printing; the fallback-only cards are all vanilla.
- **Status:** resolved: the canonical-printing fallback never decides a non-vanilla card. Rust and Python pick the same canonical printing for every card.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w116_the_canonical_fallback_never_decides_a_non_vanilla_card` (a synthetic `_p10`/`_p2` pair trips it) and `carddefs.rs::every_card_def_equals_python_field_by_field` (T-035).
- **Ablation:** no AI effect (row K).

### W-117 — The adversarial review was a sample
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/SPEC.md:64`, `docs/PROGRESS.md:55-68`, `src/gcg_sim/tools/fuzz.py:33-43`
- **Quotes:** “stratified rules sample (54 rules), 23 N/A reasons, 64 cards”
- **What is weak:** 64 of 1,152 cards and 12 replays were audited; the other cards' behaviour rests on tests and goldens.
- **Resolution plan:** for the port, the differential check is exhaustive instead of sampled. Every card's golden record and tagged tests run in both implementations. Differential fuzz uses generated decks that jointly contain every card a deck can hold, which is 1,064 numbers: 956 coloured main-deck cards and 108 Resources. The other 88 numbers (29 UNIT TOKEN, 30 EX BASE, 29 EX RESOURCE) cannot be in a deck. They are covered by their golden records and tagged tests, and by fuzz games in which an effect creates them. The fuzz execution record (W-151) shows which programs actually ran, because a card being in a deck does not mean its program ran.
- **Verification** (e2e): the differential report has zero differences, split into four parts. Golden records and tagged tests: 1,152/1,152 numbers. Deck membership in differential fuzz: 1,064/1,064. Execution in differential fuzz: per card with a program, run or not, every card that never ran listed with the test that covers it instead. Effect-created token and EX numbers: created in fuzz, or covered by a golden record and a tagged test (88/88).
- **Status:** resolved: `gcg-diff coverage --check` prints the four-part differential report, and any card without its evidence fails the check.
- **Evidence:** `gcg-diff coverage --check` passes (T-093):
  - part 1: 1152/1152 numbers with a golden record and Rust evidence;
  - part 2: 1064/1064 (10,000 games, seed 2026, 0 failures);
  - part 3: 708 of 724 pool cards with a program ran, and the 16 others each have a passing tagged trace;
  - part 4: 88/88 effect-only numbers.

  Unit test `coverage::tests::w117_an_uncreated_effect_only_number_needs_a_trace_an_identical_printing_or_a_tag`.
- **Ablation:** no AI effect (row N).

### W-118 — verify_all.sh gate limits
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `scripts/verify_all.sh:2-5`, `scripts/verify_all.sh:19-30`, `scripts/verify_wheel_offline.sh:10`, `scripts/verify_wheel_offline.sh:26`, `scripts/verify_wheel_offline.sh:34-38`, `.claude/skills/gcg-refresh-data/SKILL.md:291`
- **Quotes:** “the full gate on a clean clone of the committed HEAD (uncommitted”
- **What is weak:** it verifies only committed HEAD; it does not run `check_doc_commands.py`, the 500-game fuzz CLI, the traceability regeneration or the research-artifact checks; the wheel is picked by `ls … | tail -n 1` (lexicographic, wrong with several wheels); the offline check uses one Python version and one pairing with 2 matches for the worker-count identity.
- **Resolution plan:** the Rust full-gate script runs on a clean clone: fmt, clippy, all test tiers, the doc checker, the fuzz CLI, traceability regeneration with a diff check, the equivalence comparison and the offline binary check with several pairings and worker counts 1/4/10; artifacts are selected by exact version, never by sort order.
- **Verification** (e2e): the gate fails when any step fails (checked by injecting a failing step in a test copy) and passes on the clean branch.
- **Status:** resolved: `scripts/verify_all_rust.sh` (and `--full`) runs every gate step on a clean clone of HEAD, in a subshell that fails on any error, and its self-test passes. The verification's "passes on the clean branch" is met: `GCG_GATE_WORKERS=4 RUST_TEST_THREADS=4 CARGO_BUILD_JOBS=4 scripts/verify_all_rust.sh --full` cloned the GAP-D branch at 54c2bd2, ran every step there and ended with "verify_all_rust passed", exit 0 (`docs/port/eval/weakness-status/GAP-D.md`, "The official clean-clone run"). GAP-D added the gate lines the reports proposed (the T-153 identity, `gcg-diff api-snapshot --check`, `scripts/throughput.py --selftest`), runs the documented commands in their own clone with `--timeout 21600`, and lets `GCG_GATE_WORKERS=N` cap the workers on a shared machine. No clean-clone run of the script at the merged HEAD is recorded; T-001 (acceptance) is the lead's. The Python gate's limits (bash 3.2 `set -u`/EXIT-trap hole, wheel picked by `ls | tail`) stay unchanged by design and are listed for the lead.
- **Evidence:**
  - `scripts/verify_all_rust_selftest.sh`: a failing step, a failing pipeline and an unset variable each fail a copy of the gate before the next step, and no step is guarded by `|| true`. `GCG_GATE_WORKERS=x scripts/verify_all_rust.sh` exits 2.
  - The official clean-clone run (GAP-D, 2026-11-03): fmt and clippy clean; default tier 1,247 passed, slow tier 1,271 passed, paranoid `gcg-engine` 247 passed; tier-1 corpus 3804/3804 in every mode; `coverage`, `traceability`, `tests-json`, `weaknesses` and `api-snapshot` checks pass; the fuzz and paranoid fuzz of W-110; the 1/4/10-worker identity runs; the T-153 identity; the offline binary check; and in `--full` the Python lints, the documented commands (19 run, 11 skipped, 0 failed), the tier-1 regeneration, the equivalence smoke (3/3) and tier 2 at 4154/4154.
  - `scripts/verify_binary_offline.sh target/release/gcg-sim`: 3 pairings, workers 1/4/10.
- **Ablation:** no AI effect (row N).

### W-119 — blind_transcripts.py uses a private helper and path-dependent ids, untested
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `scripts/blind_transcripts.py:32`, `scripts/blind_transcripts.py:113-121`
- **Quotes:** “from gcg_sim.runner.replayfile import _decklist, describe_action”
- **What is weak:** the script imports the private `_decklist`, its game ids hash the actions plus the file path (they change with the directory layout), and no test covers it.
- **Resolution plan:** the Rust port (gcg-eval `transcripts`, used by W-17) uses public replay APIs, derives ids from the replay content only, and has a test.
- **Verification** (unit): the same replay in two directories gets the same id; transcripts contain no search statistics.
- **Status:** resolved: the Python script is fixed and tested. `scripts/blind_transcripts.py` uses the public replay API, and its ids are derived from the game alone. The Rust port of the plan is `gcg-eval transcripts OUT_DIR KEY_FILE --run RUN_DIR [--run RUN_DIR ...]` (`crates/gcg-eval/src/transcripts.rs`; `docs/port/eval/weakness-status/GAP-C.md`), over the public replay API and the engine's public view. Every replay under a run directory must reproduce. Ids are the first 12 hex digits of the SHA-256 of the game fields and both decklists in Python's canonical JSON, and the key file is Python's. Deviation: the run directories are a repeatable `--run` option, because the shared argument parser has no `nargs='+'` positional. W-17's audit can use either tool.
- **Evidence:**
  - `uv run python scripts/blind_transcripts.py --selftest`. The same replay in two directories gets one id, and transcripts hold no search statistics.
  - `crates/gcg-eval/tests/gapc_tools.rs::w119_one_id_per_game_wherever_it_lives_and_no_search_statistics` (pass): the fixture replay in two directories, one copy with a decision log and agent names added, gets one id; the transcript holds none of the words `visits, iterations, mean, alternatives, mcts, search, greedy, agent`; the id ignores agents and logs and changes with the actions.
  - `gapc_tools.rs::w119_a_replay_that_does_not_reproduce_is_refused` (pass): a tampered replay exits 1 with no key file; a missing `--run` exits 2.
  - GAP-C found that `scripts/blind_transcripts.py:98` compares the card type with `"ex_resource"`, while `CardType.EX_RESOURCE`'s value is `"EX RESOURCE"`, so every Python transcript prints `EX 0`. With that line fixed in a scratch copy, the Python and Rust transcripts of a 2-match run were byte-identical. The committed line is unchanged at `a926ac7`, so the fix (`"EX RESOURCE"`) is listed for the lead.
- **Ablation:** no AI effect (row N).

### W-120 — Project Claude permissions are broader than the "only the refresh skill uses the network" rule
- **Category:** tooling · **Resolution type:** configuration change (needs the user's approval)
- **Sources:** `.claude/settings.json:5`, `.claude/settings.json:13-15`, `CLAUDE.md:60-61`
- **Quotes:** “"Bash(curl -sS *)",”; “It is the only workflow that uses the network.”
- **What is weak:** `curl` to any host, `python3 *` and `uv *` are allowed project-wide while the project rules restrict network use to `/gcg-refresh-data`; `Bash(git -C * )` has a trailing space, so it may not match `git -C path cmd`.
- **Resolution plan:** proposed change, to be applied only with the user's approval (configuration is outside the port's files): replace the generic `curl` rule with the existing `WebFetch(domain:…)` entries or host-specific curl patterns, and remove the trailing space from the `git -C` rule.
- **Verification** (unit): a check that parses `.claude/settings.json` and fails on a host-unrestricted network command or a pattern with trailing whitespace.
- **Status:** open, pending the user. PLAN §0.1, A32 (3), a lead decision: "Deferred to the user, asked once near the end of the port (it changes this project's Claude permission settings, which only the user may decide). Nothing else depends on it". Done in scope: `scripts/check_claude_settings.py`, which only reads. On the current `.claude/settings.json` it reports two findings and exits 1: `Bash(git -C * )` has edge whitespace in its pattern, and `Bash(curl -sS *)` lets curl reach any host. Its `--selftest` passes, and the gate runs it. The settings file is unchanged.
- **Evidence:** `python3 scripts/check_claude_settings.py --selftest`. The self-test includes the settings as the proposed change leaves them.
- **Ablation:** no AI effect (a configuration check).

### W-121 — Refresh tooling: only the offline core is tested; official fetches need a TLS workaround
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `5fc3f8b`, `docs/ASSUMPTIONS.md:47-48`, `6078099`
- **Quotes:** “refresh.py is the offline, tested core; skill scripts fetch (git sparse clone, official pages)”; “the macOS bundle that uv-managed Pythons read lacks GlobalSign Root R46”; “Refresh skill: verify official-site TLS with certifi's CA bundle”
- **What is weak:** the network fetch and verify scripts are outside the tested core; `fetch_official.py` depends on certifi's CA bundle because the macOS bundle lacks GlobalSign Root R46.
- **Resolution plan:** the refresh workflow stays in Python (data is not ported); the port adds a Rust data-loader test against recorded fixtures of every fetched file format, so a refresh that changes a format fails in Rust CI before it ships; fetch scripts get recorded-response tests on the Python side, handed to the lead.
- **Verification** (unit): the Rust loader accepts the recorded fixtures and rejects a fixture with a renamed field with a clear error.
- **Status:** resolved: the refresh workflow stays in Python. On the Rust side, recorded `cards.ndjson` records load through the public `CardDb::from_sources`, and a renamed field fails, naming the file and the field. On the Python side, recorded-response tests cover news discovery (W-153). TLS still verifies against certifi's bundle.
- **Evidence:** `crates/gcg-diff/tests/p6n_tooling.rs::w121_the_card_loader_accepts_recorded_records_and_names_a_renamed_field`.
- **Ablation:** no AI effect (row N).

### W-122 — Untracked bytecode of an unmerged later branch (bench/, tests/bench, tests/perf)
- **Category:** tooling · **Resolution type:** tooling (inventory)
- **Sources:** `bench/__pycache__`, `bench/legacy_ai/__pycache__`, `bench/perf/__pycache__`, `tests/bench/__pycache__`, `tests/perf/__pycache__`, `tests/determinism/__pycache__`, `bench/__pycache__/weakness.cpython-315.pyc`, `bench/__pycache__/ablate.cpython-315.pyc`, `src/gcg_sim/ai/__pycache__/inference.cpython-315.pyc`, `src/gcg_sim/ai/__pycache__/lethal.cpython-315.pyc`, `src/gcg_sim/engine/__pycache__/viewdeps.cpython-315.pyc`
- **Quotes:** “The weakness benchmark (plan section 6.7): legacy ``standard`` against new ``standard``, per”; “Ablations of the SO-ISMCTS flag families”
- **What is weak:** `bench/` holds only `.pyc` files (ablate, calibrate, differential, fidelity, h2h, weakness, legacy_ai, perf and others), plus `tests/perf`, `tests/bench` and `tests/determinism`; none appear in git history and all are gitignored (`__pycache__/`, verified this session); they document abandoned work (SO-ISMCTS rewrite, `gcg_sim.ai.lethal`, `gcg_sim.ai.inference`, `engine/viewdeps.py`) that does not exist on this branch.
- **Resolution plan:** treat the bytecode as read-only evidence only (strings already mined into W-17, W-21, W-48, W-54, W-100, W-114, W-125, W-126); the port never imports or runs it. Deleting the untracked directories is a destructive step left to the user.
- **Verification** (unit): the Rust build and gate scripts never reference `bench/` or `__pycache__` (a grep test).
- **Status:** resolved by the lint (PLAN §0.1 A32 (6), §12.10): the bytecode is kept, and deleting it is the user's option.
- **Evidence:** `target/release/gcg-diff lints` passes, with rule 6 and its fixture test `crates/gcg-diff/tests/lints.rs`. `crates/gcg-diff/tests/p6n_tooling.rs::w122_the_p6n_gate_scripts_never_name_the_untracked_bytecode`.
- **Ablation:** no AI effect (row N).

### W-123 — compare.py usability limits
- **Category:** tooling · **Resolution type:** eval tooling
- **Sources:** `src/gcg_sim/ai/compare.py:14-16`, `src/gcg_sim/ai/compare.py:58`, `src/gcg_sim/ai/compare.py:70-76`, `src/gcg_sim/ai/compare.py:89-93`, `src/gcg_sim/ai/compare.py:166-171`
- **Quotes:** “Decks: ``tests`` (``tests/ai/decks.py``, run from the repository root)”
- **What is weak:** the default deck set `tests` is unavailable from an installed wheel; decks are labelled `deck0`/`deck1` in first-seen order; override converters cover only int/float/str/bool fields; baselines exist only for `standard` and `strong` (no baseline `fast`).
- **Resolution plan:** gcg-eval embeds its deck sets (test decks, tuning decks, panel, examples) in the binary, labels decks by file stem or deck name, parses every config field type with a typed parser, and exposes reference `fast`, `standard` and `strong` as named agents.
- **Verification** (unit + e2e): the installed binary runs with the default deck set from any directory; every config field round-trips through the parser; output labels show deck names.
- **Status:** resolved: `build.rs` embeds the example and evaluation decks for use from any directory. Reports label decks by file stem. `ref:*`, `search:null-<preset>` and the baselines are named agents. A `search:` override may name any configuration field. `compare` keeps Python's behaviour (T-079).
- **Evidence:**
  - `agent_spec::tests::w123_every_config_field_round_trips_through_the_parser` (24 fields).
  - `null_heads_carry_the_reference_settings`.
  - `decks::tests::built_in_examples_are_the_files`.
  - `crates/gcg-eval/tests/phase6.rs::w123_named_deck_sets_work_from_any_directory`.
- **Ablation:** no AI effect (row I).

## Performance

### W-124 — Search is engine-bound; invalidating the derived view by mutation kind is deferred
- **Category:** performance · **Resolution type:** Rust fix (output-identical)
- **Sources:** `docs/python/AI.md:280-299`, `docs/python/AI.md:497-502`, `docs/python/AI.md:510`, `docs/ARCHITECTURE.md:50-51`, `docs/ARCHITECTURE.md:76-81`, `docs/ASSUMPTIONS.md:93`, `README.md:110-116`, `docs/status.json:48`, `src/gcg_sim/ai/config.py:93-97`, `d3f8dd6`, `c6716f5`, `d9d41af`
- **Quotes:** “Invalidating the derived view by mutation kind (74% of recomputations produce an identical view) needs a dependency analysis of every condition and is deferred.”; “In a profiled game on the pre-change engine, 80–83% of search time was inside the engine's `apply`”; “28.2 vs 2.90 (9.7x)”; “Engine performance: per-zone ability lists, single base view per derived computation, event-indexed triggers (1.7x throughput)”
- **What is weak:** games per core-minute: `fast` 28.2, `standard` 2.90 (20.7 core-s per game), `strong` 1.82, pre-change `standard` 4.72; 100 BO3 matches on 8 workers took 13.0 min. The speedups cut calls by about a third, view recomputations by 40% and determinizations by 73% (28,134 to 7,627 per game); the derived view is a fixpoint recomputed after any mutation and 74% of recomputations produce an identical view. Pre-#5 profile: 91% in `apply`, 69% in `view._compute`.
- **Resolution plan:** in Rust: (1) static dependency masks for every condition and filter computed from the IR (which state facets each reads), and invalidation by mutation kind, with conditions the analysis cannot see through (custom steps that call execute) marked "dynamic" and always recomputed; (2) arena-allocated, cache-friendly state with cheap clones; (3) the paranoid feature (W-49) compares every masked skip with a full recomputation.
- **Verification** (e2e + unit): output identity (games, decision logs, results) with masks on and off over the equivalence seeds; paranoid fuzz with zero mismatches; throughput measured in node-count-normalised units (W-126) against the Python baseline.
- **Status:** resolved: Phase 5 did the output-identical view work. Per-mutation-kind invalidation masks are not adopted, a decision recorded in `docs/port/eval/weakness-status/P6-J.md`. They would need a dependency analysis of every condition, filter and value, the view and step hooks are opaque to one, and the measured speed already clears the floor. The work, measured in instructions retired per standard game:
  - the sparse derived view in recycled buffers: 14.75G to 6.75G;
  - then down to 4.65G, including the skip of the confirming pass, with a `paranoid` skipped-pass check (105,974 runs).

  `view::derived` fell from 62.5% to 43.3% of the samples. M10 timing: Part 1 headline 23.5x, Part 2 `standard` full search 2.66x; final timing (8c89b5e, with the M15b mechanisms): 23.5x and 2.40x (`docs/port/BENCHMARKS.md`).
- **Evidence:** `docs/port/BENCHMARKS.md` (the Phase 5 log); the paranoid checks; tier 1 3804/3804 and tier 2 4154/4154.
- **Ablation:** no AI effect (row J). Phase 5's changes are output-identical by the same corpus evidence.

### W-125 — Action and Decision are frozen dataclasses (slow construction on the hot path)
- **Category:** performance · **Resolution type:** Rust fix (by construction)
- **Sources:** `src/gcg_sim/engine/state.py:14-15`, `src/gcg_sim/engine/state.py:35-36`, `tests/perf/__pycache__/test_engine_values.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “``Action`` and ``Decision`` are plain (not frozen) dataclasses for construction speed”
- **What is weak:** on this branch both are frozen dataclasses built in the hot path (the lost perf branch made them plain with value semantics and a fast `copy()` for `GameState.clone`).
- **Resolution plan:** in Rust `Action` is a small `Copy` value type and `Decision` holds its options in a reusable buffer; equality and hashing are by value.
- **Verification** (unit): equality semantics tests (two actions with the same fields are equal and hash equal); micro-benchmark recorded in the port's benchmark report.
- **Status:** resolved by construction (Phase 3): `Action` is a 16-byte `Copy` value, and the pending decision is a `Copy` header plus a reused options buffer. Deviation: no micro-benchmark was added. The brief forbids benchmark code in the crates, so the external comparison in `docs/port/BENCHMARKS.md` is the only measurement.
- **Evidence:** `crates/gcg-engine/tests/p6j_engine.rs::w125_actions_compare_and_hash_by_value`.
- **Ablation:** no AI effect (row J).

### W-126 — Throughput measurement counts wall time x workers on one pairing; machine state is not recorded
- **Category:** performance · **Resolution type:** tooling
- **Sources:** `src/gcg_sim/ai/throughput.py:5`, `src/gcg_sim/ai/throughput.py:21`, `src/gcg_sim/ai/throughput.py:33-38`, `bench/__pycache__/timing.cpython-315.pyc`, `bench/perf/__pycache__/micro_compare.cpython-315.pyc`, `bench/perf/__pycache__/ab.cpython-315.pyc`
- **Quotes:** “core_minutes = wall * workers / 60.0”; “Runs are not gated on a quiet machine”
- **What is weak:** process spawn, imports and idle tail workers count as busy core time; only `TUNING_DECKS[0]` vs `[1]` is measured; the documented `--games 26` is not a multiple of the 4-game block; machine load and power state are not recorded; the lost harness used the minimum over runs and a 1.03 regression ratio.
- **Resolution plan:** the port's throughput tool (outside the deterministic crates, as `scripts/compare_python_vs_rust.sh` already is) reports CPU time per game from the OS, nodes and decisions per second (deterministic counts over measured time), several deck pairings, whole blocks only, the minimum and median of repeated runs, and the machine's load and power state.
- **Verification** (e2e): two runs on an idle machine agree within 3% on the minimum; a `--games` value that is not a multiple of 4 is rounded up with a message.
- **Status:** open: `scripts/throughput.py` is outside the crates. It times `gcg-eval head-to-head` on several pairings in whole blocks of four, repeated `--runs` times. It reports wall time, CPU time from `getrusage`, deterministic effort counts per second, load average, CPU, core tiers and memory. It reads no power state, because the brief's operating assumption overrides that part of the plan. `scripts/throughput.py --agree FIRST.json SECOND.json [--tolerance 0.03]` checks two reports of the same command against the criterion, and the gate runs its self-test (`docs/port/eval/weakness-status/GAP-D.md`). Remaining: the pair of `scripts/throughput.py` runs on an idle machine that the verification's "two runs on an idle machine agree within 3% on the minimum" needs has not happened, or the lead's record that the harness sessions below meet the criterion.
- **Evidence:**
  - `python3 scripts/throughput.py search:fast --opponent ref:fast --games 3 --runs 2 --pairs red-green-zeon:blue-white-federation --workers 2`: "--games 3 rounded up to 4", with identical effort counts in both runs.
  - `scripts/throughput.py --selftest` (in the gate) covers block rounding, the summary minima, agreement at 2.5% and 2%, and failure at 5%, on different effort counts and on different commands.
  - A tool check on the loaded machine (load average 7.0, so not evidence for the criterion): two sessions on two pairings agreed within 1.08% on the minima, "throughput runs agree", exit 0 (`docs/port/eval/GAP-D/throughput-A.json`, `throughput-B.json`).
  - The `scripts/compare_python_vs_rust.sh` harness on the idle machine: the two sessions at 34b3107 have medians within 0.35% (GAP-D), and the final timing session at 8c89b5e recorded minima of 47.8 s and 28.0 s for the reference `standard` at m100 w10 and m20 w1, against 47.8 s and 28.1 s at M10 (`docs/port/BENCHMARKS.md`, Part 1; the reference search is unchanged between the two commits). These are the harness's numbers, not `throughput.py`'s.
- **Ablation:** no AI effect (row N).

## Docs

### W-127 — Investigation evidence is not reproducible: raw outputs were gitignored and cited commits were squashed away
- **Category:** docs · **Resolution type:** docs + eval tooling
- **Sources:** `docs/PROGRESS.md:96-103`, `docs/PROGRESS.md:109-110`, `docs/PROGRESS.md:117`, `docs/PROGRESS.md:126`, `docs/PROGRESS.md:134-163`, `docs/python/AI.md:280`, `docs/python/AI.md:345`, `d9d41af`, `23ddd49`
- **Quotes:** “Raw outputs under `build/aiq2/` (gitignored).”; “Measured on commit fc29d5f”
- **What is weak:** `build/aiq2/` (arms files, audit transcripts, `classified.md`) is absent from this checkout; `git cat-file` fails for 5a6f226, fc29d5f, daf73c5, c27e523, ed2cedc, 4be24da, 87d8a72, 65d6e1d, 9684f29, 8eb9e8a, a339080, 3351b78, 39c5643, d79ea78, a32d212 and 4d8bd95 (verified this session), all squashed into d9d41af; 99 commits exist, all reachable from `main`, and the tag `baseline` points at d9d41af.
- **Resolution plan:** for the port, every number used as evidence is produced by a committed gcg-eval command whose raw output (game records, ablation rows, traces) is committed in a compressed evidence directory with the exact command and commit; Rust docs cite only reachable commits, checked by a doc test. The Python docs' dangling shas are listed for the lead (Python docs are outside the port's files).
- **Verification** (unit): a doc check resolves every commit sha in the port docs with `git cat-file -e` and every evidence file path exists.
- **Status:** resolved (port side): `gcg-eval evidence add|check` manages a manifest of command, commit, dirty flag and SHA-256. The check resolves every cited commit and evidence path under `docs/port/eval/`. The Python docs' dangling shas (5a6f226, fc29d5f, daf73c5, c27e523, ed2cedc, 4be24da, 87d8a72, 65d6e1d, 9684f29, 8eb9e8a, a339080, 3351b78, 39c5643, d79ea78, a32d212, 4d8bd95) are listed for the lead.
- **Evidence:**
  - `crates/gcg-eval/tests/phase6.rs::w127_evidence_check_resolves_commits_and_paths`.
  - `w127_the_port_evaluation_docs_cite_only_reachable_commits`.
  - `evidence::tests::commits_and_paths_are_found_in_markdown`.
- **Ablation:** no AI effect (row I).

### W-128 — Stale counts across status.json and the research docs
- **Category:** docs · **Resolution type:** docs (generated counts)
- **Sources:** `docs/status.json:3`, `docs/status.json:18-33`, `README.md:8`, `docs/CONFLICTS.md:90`, `docs/PROGRESS.md:20-21`, `docs/research/RULES_INDEX.md:24-25`, `docs/research/RULES_INDEX.md:45`, `docs/research/RULES_INDEX.md:79`, `docs/research/RULES_INDEX.md:86`, `docs/RULES_TRACEABILITY.md:5`, `docs/research/EFFECT_TEMPLATES.md:15`, `docs/research/EFFECT_TEMPLATES.md:19-23`, `docs/research/EFFECT_TEMPLATES.md:225`, `docs/research/work_packages.json:7-8`, `docs/research/DATA_INGESTION.md:7-9`, `docs/research/DATA_INGESTION.md:22-23`, `docs/research/DATA_INGESTION.md:145-147`, `docs/research/data_profile.json:2851`, `docs/research/data_profile.json:3478`, `docs/research/OFFICIAL_SOURCES.md:204`, `src/gcg_sim/data/official/edition_language.json:9`, `data/sources.d/gcgapi.json`
- **Quotes:** “"card_numbers": 1148”; “the 399 ids that need a @pytest.mark.rule test”; “At this snapshot there are 1,148 card numbers (1,912 printings).”; “"data: weekly refresh (run 25)"”; “Card data holds 85 Edition Beta printings.”
- **What is weak:** `status.json` (dated 2026-10-18) says 1148/1148 cards and 395 conflicts while README says 1152/1152 and CONFLICTS 402; RULES_INDEX says 399 testable / 180 N/A against traceability's 395 / 184 (7a797fb reclassified four) and says the CR PDF was kept only in the scratchpad; EFFECT_TEMPLATES prose says 1,148 numbers / 1,912 printings / 308 vanilla / 189 binding cards while its generated blocks and `work_packages.json` say 1,152; DATA_INGESTION and `data_profile.json` profile run 25 (1,912 printings) against the packaged run 28 (2,011); Edition Beta counts are 83 printings / 70 numbers vs 85.
- **Resolution plan:** every count in the port's docs is generated from the packaged data or test results by a docs tool and checked by a test (counts cannot be hand-typed); the list of stale Python-doc counts above goes to the lead for the Python side.
- **Verification** (unit): the port docs' counts test regenerates every count and fails on any difference.
- **Status:** resolved for the port's documents: `scripts/doc_counts.py` computes 14 named counts from the data and checks every `<!-- count:NAME -->` marker in `docs/port/**/*.md`, and the gate runs `--check`. The stale Python-doc counts are listed for the lead (P6-N lead note 6).
- **Evidence:** `crates/gcg-diff/tests/p6n_tooling.rs::w128_marked_counts_are_regenerated_from_the_data`.
- **Ablation:** no AI effect (row N).

### W-129 — Hand-maintained docs drift from the code
- **Category:** docs · **Resolution type:** docs (checked contracts)
- **Sources:** `docs/SPEC.md:18`, `pyproject.toml:9`, `docs/ASSUMPTIONS.md:75-78`, `docs/INTERFACES.md:64`, `docs/INTERFACES.md:143-149`, `docs/DECK_FORMAT.md:88`, `src/gcg_sim/deck/validate.py:268`, `docs/PROGRESS.md:136`, `src/gcg_sim/ai/selfplay.py:95-104`, `docs/python/AI.md:326-327`, `docs/python/AI.md:309-310`, `15ae2eb`, `32e9f84`, `99f43fe`, `ceb7d1d`
- **Quotes:** “Python ≥3.12”; “def validate_deck(deck: Deck, *, banlist: bool = True) -> list[Violation]”; “values still provisional”; “SPEC, CLAUDE.md, skill, DECK_FORMAT, REPORTS, AI and marker docs corrected”; “gcg-benchmark skill uses the real CLI”; “Blocks of four games on one game seed covering every (seat, deck) assignment”; “means blocks of four games per deal”
- **What is weak:** SPEC says Python ≥3.12 while the package requires ≥3.15. The "frozen" INTERFACES contract lacks `implemented=True` and omits `replay --log` and `data status --json`. PROGRESS still calls the `fast` values provisional. Review commits corrected doc drift repeatedly, and the benchmark skill was drafted before the CLI existed. The `mirrored_specs` docstring ("Blocks of four games on one game seed") and `docs/python/AI.md:326-327` ("blocks of four games per deal") are also wrong: the code derives the game seed from the block and the deck order, so each block of four holds two deals of two games each (W-42). `docs/python/AI.md:309-310` ("each deal with the challenger on both decks and in both seats") is correct. The error matters for any paired analysis (W-42, W-43).
- **Resolution plan:** the port's interface docs are generated from the Rust public API (`cargo doc` plus a CLI `--help` snapshot test), so signatures and subcommands cannot drift. The gcg-eval docs describe a mirrored block as two deals, each played with the agents swapped, and a test checks that description against the ported generator. The Python-doc corrections above, including the `mirrored_specs` docstring and `docs/python/AI.md:326-327`, go to the lead.
- **Verification** (unit): snapshot tests of `--help` output for every subcommand and of the public API signatures fail on any undocumented change. A unit test checks the ported mirrored-block generator: every block of four uses exactly two game seeds, each in two games with the agents in opposite seats, as the gcg-eval docs state, and the seeds equal Python's `mirrored_specs` on fixed master seeds.
- **Status:** resolved:
  - `scripts/cli_help_snapshot.py` snapshots the `--help` of `gcg-sim`, `gcg-eval`, `gcg-diff` and every subcommand (25 texts) into `docs/port/eval/P6-N/cli-help.txt`, and the gate fails on a change.
  - `gcg-diff api-snapshot` lists the public API of the library crates from the sources, on the stable toolchain (rustdoc JSON needs nightly), and `--check` fails on a change (`docs/port/eval/weakness-status/GAP-D.md`).
  - The `docs/SPEC.md`, `docs/INTERFACES.md` and `docs/PROGRESS.md` corrections are done.
  - D14a is done: the Python text is at `docs/python/AI.md` (moved with `git mv`), every citation of it names that path, and the Rust `docs/AI.md` holds the preset table, the mapping to the reference presets and the mechanism table. `crates/gcg-search/tests/ai_md_presets.rs` (T-130) compares those tables with `crates/gcg-search/src/presets.rs` and the reference presets, and fails on any difference.
  - The mirrored-block wording of `docs/python/AI.md:309-310` and `:326-327` now says two deals per block of four, each played twice with the agents swapped (the quote above is the text before that correction).
- **Post-acceptance:** the `mirrored_specs` docstring (`ai/selfplay.py:95-97`), per §12.11 answer (a).
- **Evidence:**
  - `python3 scripts/cli_help_snapshot.py --check` passes.
  - `gcg-sim`'s help equals Python's (T-152).
  - The mirrored-block statement is checked by `crates/gcg-eval/tests/vectors.rs::mirrored_specs_match_selfplay`.
  - `target/release/gcg-diff api-snapshot --check` passes.
  - `cargo test --release -p gcg-search --test ai_md_presets` passes (4 tests); with a budget, a setting or a mechanism of the doc tables changed, 3 of them fail.
- **Ablation:** no AI effect (row N).

### W-130 — The shuffle-cut and chooser-procedure wording disagree with the engine and REPORTS
- **Category:** docs · **Resolution type:** port-preserve + docs
- **Sources:** `docs/research/OFFICIAL_SOURCES.md:140-141`, `src/gcg_sim/engine/game.py:116-117`, `src/gcg_sim/data/official/floor_rules.json:34`, `docs/REPORTS.md:36`
- **Quotes:** “The confirming cut or shuffle is not modelled, because it leaves a uniform permutation uniform.”; “Seeded fair coin flip picks the chooser.”
- **What is weak:** the research doc says the confirming cut is not modelled, while `new_game` shuffles with the seeded RNG and then cuts at a random point (Floor Rules), and the RNG consumption order must match for replay; `floor_rules.json` says a seeded fair coin flip picks the chooser while REPORTS says a seeded die roll.
- **Resolution plan:** Rust reproduces the engine exactly (shuffle, then cut, same RNG draws) and its docs describe what the engine does; the wording mismatch in the Python docs and the packaged data file is listed for the lead (packaged data stays unchanged in the port).
- **Verification** (unit): RNG-stream test: Rust `new_game` consumes the same SplitMix64 draws as Python for a set of seeds (same deck order and uids).
- **Status:** resolved (docs): `docs/research/OFFICIAL_SOURCES.md` now describes the seeded shuffle and cut, and `docs/REPORTS.md` the seeded fair draw (`randrange(2)`) that picks the chooser. `src/gcg_sim/data/official/floor_rules.json` stays unchanged as packaged data and is listed for the user. Its "fair coin flip" is accurate.
- **Evidence:** `crates/gcg-engine/tests/l5_setup.rs::new_game_shuffles_cuts_and_numbers_cards_as_python`: Rust `new_game` gives Python's digest, RNG state, card order and chooser for every exported seed (T-151).
- **Ablation:** no AI effect (row N). Documents only.

## Design decisions

### W-131 — Multiplayer (rules section 12) is out of scope
- **Category:** design-decision · **Resolution type:** admissible exception
- **Sources:** `README.md:164`, `docs/SPEC.md:45`, `docs/ASSUMPTIONS.md:57`, `docs/research/RULES_INDEX.md:39`, `docs/research/RULES_INDEX.md:77`, `docs/research/RULES_INDEX.md:92`, `docs/research/OFFICIAL_SOURCES.md:48-50`, `docs/research/EFFECT_TEMPLATES.md:890-902`, `docs/CONFLICTS.md:5438-5450`, `src/gcg_sim/effects/compiler/grammar.py:8-10`, `src/gcg_sim/effects/compiler/grammar.py:66-67`, `src/gcg_sim/effects/bindings/wp_eb01_a.py:3`, `src/gcg_sim/effects/bindings/wp_eb01_a.py:334`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:4-6`, `src/gcg_sim/rules/index.py:6`, `src/gcg_sim/data/rules/rules_na.json:516`, `tests/meta/rulings_na/wp_eb01_a.json`, `tests/meta/rulings_na/wp_st09_11.json`, `e240669`
- **Quotes:** “**1v1 only.** Multiplayer (rules section 12) is out of scope; its rules are marked N/A.”; “"2 or more enemy players" is false: EB01-040, EB01-044, EB01-055 and EB01-058 then do”; “1v1 interpretation of multiplayer wording (rule 12 is out of scope)”; “Multiplayer Battle | 32 | 1 | 31 | 32 | 0”
- **What is weak:** all 32 section-12 rules are N/A; multiplayer wording is read for 1v1 ("number of enemy players" = 1, "each enemy player"/"another player" = the opponent, ST10-001's "enemy team" = the opponent), so EB01-040/044/055/058 never fire; 8 rulings are N/A as multiplayer-only (EB01-005:Q312, EB01-009:Q314, EB01-074:Q330, GD05-034:Q486, ST10-001:Q302, ST10-008:Q305, ST10-008:Q306, ST14-001:Q464); the multiplayer formats Team Battle and Battle Royale are unsupported. Sealed Battle is a two-player limited deck-construction format, not multiplayer, so it is not part of this exception (W-76).
- **Resolution plan:** none required: an explicit design decision of the reference. The port keeps the 1v1 readings exactly, and the multiplayer N/A list is ported as data so the traceability report stays complete.
- **Verification** (n/a): exception; the 1v1 readings are covered by the ported tagged tests (W-56).
- **Status:** admissible exception (design decision): the reference simulates two-player Standard play only, and multiplayer was explicitly scoped out. No work.
- **Evidence:** n/a (exception). The 1v1 readings are covered by the ported tagged tests (W-56) and the multiplayer N/A list in `src/gcg_sim/data/rules/rules_na.json` (`gcg-diff traceability --check`, T-094).
- **Ablation:** no AI effect. It is an admissible exception, and no code changed.

### W-132 — No sideboarding in BO3
- **Category:** design-decision · **Resolution type:** admissible exception
- **Sources:** `README.md:53`, `README.md:165`, `docs/research/OFFICIAL_SOURCES.md:174-176`, `docs/research/OFFICIAL_SOURCES.md:255-259`, `docs/CONFLICTS.md:966-973`, `docs/DECK_FORMAT.md:72-73`, `src/gcg_sim/data/official/banlist.json:30`, `src/gcg_sim/data/official/banlist.json:34`, `src/gcg_sim/data/official/deck_construction.json:118`, `src/gcg_sim/data/official/bo3_match_rules.json:19`, `src/gcg_sim/runner/match.py:19-37`
- **Quotes:** “**No sideboarding or clock** in BO3, as the task specifies.”; “the simulator uses the main deck only; no sideboard”
- **What is weak:** the official BO3 page allows a 10-card sideboard (4-copy and 2-colour limits count across deck and sideboard; the TRM says "No side decks are permitted"); the simulator ignores both, plays the same deck every game, and applies B&R and banned pairs to the main deck only.
- **Resolution plan:** none required: an explicit design decision of the reference ("as the task specifies").
- **Verification** (n/a): exception.
- **Status:** admissible exception (design decision): BO3 is modelled without sideboarding, by specification. No work.
- **Evidence:** n/a (exception).
- **Ablation:** no AI effect. It is an admissible exception, and no code changed.

### W-133 — No clock: time limits, extra turns and tiebreaks are not modelled
- **Category:** design-decision · **Resolution type:** admissible exception
- **Sources:** `README.md:165`, `docs/research/OFFICIAL_SOURCES.md:169-178`, `docs/research/OFFICIAL_SOURCES.md:233`, `.claude/skills/gcg-refresh-data/references/sources.md:74`, `src/gcg_sim/data/official/bo3_match_rules.json:16-21`, `src/gcg_sim/data/official/floor_rules.json:4`, `src/gcg_sim/data/official/floor_rules.json:128-134`, `src/gcg_sim/runner/match.py:19-37`, `src/gcg_sim/runner/game.py:42-51`
- **Quotes:** “The simulator also ignores time limits and extra turns.”; “no clock in the simulator.”
- **What is weak:** officially a BO3 has 60 minutes, then 3 extra turns if time runs out in Player One's turn or 2 in Player Two's, then tiebreaks (the Asia floor rules end with rock-paper-scissors). Time-related penalties are not modelled either, and a match ends after at most `games_per_match` games. Intentional draws, concessions and other penalties are tournament procedures, not part of the clock, so they are covered by W-76, not by this exception.
- **Resolution plan:** none required: an explicit design decision of the reference. Games are bounded by the engine safety caps instead (W-46).
- **Verification** (n/a): exception.
- **Status:** admissible exception (design decision): simulated games have no wall-clock dimension by design, and budgets and determinism forbid one. No work. Games are bounded by the engine safety caps (W-46).
- **Evidence:** n/a (exception).
- **Ablation:** no AI effect. It is an admissible exception, and no code changed.

### W-134 — Both players know both exact decklists
- **Category:** design-decision · **Resolution type:** AI (strengthened search, opt-in information mode)
- **Sources:** `docs/ASSUMPTIONS.md:62`, `docs/ASSUMPTIONS.md:90`, `docs/python/AI.md:30-31`, `docs/python/AI.md:200`, `docs/ARCHITECTURE.md:88-91`, `src/gcg_sim/engine/observe.py:3`, `32e9f84`
- **Quotes:** “Both AI players know both exact decklists (main and resource deck)”; “Both decklists are known to both players (a benchmarking assumption, docs/ASSUMPTIONS.md);”; “decklist knowledge recorded as an assumption”
- **What is weak:** a benchmarking choice, not a game rule (the floor rules keep main-deck contents private); determinization resamples hidden cards from the opponent's decklist minus seen cards and the `bursts` feature reads the known decklist, so the AI plays with information a real player may lack (the benchmark deck is a known meta list, the deck under test may not be).
- **Resolution plan:** keep the default (and the reference) unchanged. Add an opt-in information mode to the strengthened search, `--opponent-list known|archetype`: in `archetype` mode the opponent's unseen cards are sampled from a declared prior (a list of candidate decklists with weights, e.g. the benchmark deck's archetype variants) updated by the cards seen (Bayesian reweighting of candidate lists), never from the true decklist. AI-only; rules and data unchanged.
- **Verification** (unit + e2e): information-set test in `archetype` mode: replacing the opponent's true decklist by another list consistent with the prior and the seen cards does not change the decision or statistics; e2e: a benchmark in `archetype` mode with a one-list prior equal to the true list reproduces `known` mode exactly.
- **Status:** resolved by measurement (not adopted): `opponent_list = archetype` draws the opponent's unseen cards from a posterior over a declared prior, never from its true decklist (`crates/gcg-search/src/mech/p6c/archetype.rs`; `docs/port/eval/weakness-status/GAP-B.md`). The prior is the 8 packaged panel deck lists with equal weights. The posterior is the multivariate hypergeometric likelihood of the opponent's seen cards, and each kept list is patched to hold them. It uses W-08's engine interface (`observe::with_decklists`, `assign_hidden`). It measured null, so the default stays `known` and both presets are unchanged. Deviation: the plan's e2e is "a benchmark in archetype mode with a one-list prior equal to the true list". A prior cannot be passed from a `gcg-eval` option to the agent without a field in the lead-owned `StrengthenedConfig` (GAP-B interface request 1), so the e2e runs at the search level over full games, and the CLI runs with the built-in prior. Not covered: pre-game decisions; the forced play/draw protocol of every row (A24) never asks for CHOOSE_FIRST.
- **Option location (A30):** `gcg-eval benchmark --opponent-list known|archetype`, and the agent-spec field `search:<preset>:opponent_list=archetype`. `gcg-sim` is unchanged.
- **Evidence:** these tests pass:
  - The information-set test, `mech::p6c::gapb_tests::w134_archetype_searches_read_no_true_decklist`: at 63 searched vector decisions, a second true state whose opponent decklist is another packaged list consistent with the seen cards gives the identical archetype search. The `known` search changes at 56 of them (the negative control).
  - The one-list-prior check, `w134_a_one_list_prior_of_the_true_list_reproduces_known_mode_on_the_vectors` (126 decisions) and `w134_a_one_list_prior_reproduces_known_mode_over_full_games` (every searched decision of two full games, bit for bit).
  - `crates/gcg-eval/tests/p6m_cli.rs::w134_opponent_list_reaches_the_search_flag`: `gcg-eval benchmark --opponent-list archetype` runs to completion.
- **Ablation:**
  - Individual (dev 22800001): 197/400 (0.492, [0.444, 0.541]), -0.015 [-0.084, +0.054], discordant 24/27, 9812 vs 9605 / 119463 vs 115978 (`docs/port/eval/GAP-B/individual-opponent_list-archetype.json`).
  - High budget (dev 22800002), the M15 `standard` mechanisms at 256 / 640 / 2560, with the mode against `known`: 199/400 (0.497, [0.449, 0.546]), -0.005 [-0.073, +0.063], 24/25, 32034 vs 32122 / 581838 vs 580772 (`high-budget-opponent_list-archetype.json`).
  - The prior holds the 8 panel decks, so on the tuning pairs the true list is a candidate: the rows measure the cost of not knowing which panel list the opponent plays.
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-135 — Information-set honesty is a convention, not enforced by the API
- **Category:** design-decision · **Resolution type:** Rust fix (by construction)
- **Sources:** `docs/INTERFACES.md:29-32`, `docs/SPEC.md:77-78`, `docs/python/AI.md:34-55`
- **Quotes:** “agents receive the full state and read only their own information set”
- **What is weak:** agents receive the true `GameState`; honesty is checked by permutation tests (25 mid-game states from greedy self-play, 3 permutations each, plus a negative control), not by types.
- **Resolution plan:** Rust agents receive an `Observation<'_>` view that exposes only the deciding player's information set (public zones, own hand, counts, known cards, own pending options) and a `determinize` method; the reference ISMCTS and greedy are ported against this view, which offers the same information, so their outputs stay identical. Only the engine and the runner hold `&GameState`.
- **Verification** (unit + compile-fail): agent code cannot name `GameState` (compile-fail test); the ported permutation test and negative control pass; reference outputs equal Python's on the equivalence seeds.
- **Status:** resolved, with the D10 deviation from the plan text recorded. The strengthened search reads only an `Observation` (`crates/gcg-search/src/observation.rs`). It holds the observer's information-set state, with no reference to the true state, and the search builds its worlds from it. `SearchAgent::choose` uses the true state only in `AgentCore::begin`, `Observation::new` and `finish` (lint §9.9 item 5). D10: the reference search and greedy keep `&GameState`, because their `derived()` calls fill the true state's view cache exactly as Python's do. So the plan's "agent code cannot name `GameState`" is met for the strengthened search's code path, not for every agent.
- **Evidence:**
  - Compile-fail doctests in `observation.rs` (T-023): private field `E0616`, private accessor `E0624`, no identity method `E0599`, no `AsRef<GameState>` `E0277`, and no outliving the decision `E0515`.
  - `observation::tests::worlds_from_the_observation_are_the_reference_worlds`: 452 states x 2 observers x 3 seeds.
  - `the_observation_depends_on_the_information_set_alone`.
  - `crates/gcg-search/tests/observation.rs::the_strengthened_decision_is_invariant_under_hidden_permutation`.
  - Lint rule 5 (`crates/gcg-diff/tests/lints.rs`).
  - The reference outputs equal Python's on the equivalence seeds (T-002).
- **Ablation:** no AI effect. Under the null configuration the strengthened search makes the reference's decisions on all 661 searched decisions of the tier-1 `search` set, the 213 vector searches and the 18 vector pre-game runs (`crates/gcg-search/tests/null_config.rs`, `null_config_search_set.rs`; T-068). The reference is unchanged (corpus at 100%).

### W-136 — SearchConfig defaults reproduce the pre-change AI; baseline presets live in code
- **Category:** design-decision · **Resolution type:** Rust fix (separate types)
- **Sources:** `src/gcg_sim/ai/config.py:27-46`, `docs/ASSUMPTIONS.md:91`, `docs/python/AI.md:274-278`, `docs/python/AI.md:321-322`
- **Quotes:** “every new `SearchConfig` field defaults to the old behaviour”
- **What is weak:** `value_scale` defaults to 2.0 and every new knob is off; the presets opt in through `adopted()`; `BASELINE_PRESETS`/`BASELINE_WEIGHTS` exist so `tests/ai/test_baseline.py` can replay a bc38ab4 game bit for bit. A config built from defaults silently gets the old AI.
- **Resolution plan:** in Rust the reference search has its own config type with the Python fields and defaults, reachable only through gcg-eval; the strengthened search has a separate config type with no defaults (every preset spells out every field), so neither can be built by accident from the other.
- **Verification** (unit): the bc38ab4 baseline game and the d9d41af presets replay bit for bit through gcg-eval; constructing a strengthened config without every field does not compile.
- **Status:** resolved: `ReferenceConfig` (in `gcg-ai-reference`) and `StrengthenedConfig` (in `gcg-search`) are separate types, with no conversion in shipped code. Neither `StrengthenedConfig` nor `MechanismFlags` has a `Default`. Presets and `gcg-eval` configuration files spell out every field.
- **Evidence:**
  - Compile-fail doctests (T-023): `..Default::default()` `E0277`, a missing `mechanisms` `E0063`, and the reference config passed where a strengthened one is expected `E0308`.
  - `null_configurations_copy_every_reference_field`.
  - `search_config::tests::round_trip_and_strictness`.
  - `crates/gcg-ai-reference/tests/baseline_game.rs::baseline_standard_replays_the_game_recorded_before_the_changes` (the bc38ab4 game).
  - `presets_vectors.rs::every_reference_configuration_equals_the_live_python_object` (the d9d41af presets; T-060).
- **Ablation:** no AI effect: types and construction only. The null configuration replays the reference (T-068).

### W-137 — The determinism contract; cross-platform identity depends on the C math library
- **Category:** design-decision · **Resolution type:** port-preserve + tests + portable math (strengthened search); the `erfc` FFI needs the lead's decision
- **Sources:** `CLAUDE.md:32-33`, `docs/ARCHITECTURE.md:106-111`, `docs/python/AI.md:23-26`, `docs/INTERFACES.md:5`, `docs/SPEC.md:44`, `src/gcg_sim/ai/policy.py:98`, `src/gcg_sim/ai/evaluation.py:303-307`, `src/gcg_sim/ai/mcts.py:199`, `src/gcg_sim/ai/mcts.py:297`, `src/gcg_sim/ai/mcts.py:314`, `src/gcg_sim/ai/setup.py:109`, `src/gcg_sim/reports/stats.py:60`, `docs/port/inventory/09-python-hazards.md:421-437`, `docs/port/inventory/09-python-hazards.md:444-459`, `docs/port/inventory/09-python-hazards.md:855-864`, `Cargo.toml:17-18`, `port/oracle/export_vectors.py:1-8`
- **Quotes:** “No wall-clock, `random`, or `hash()` of strings in game logic.”; “Determinism: all randomness flows from explicit seeds through a platform-independent PRNG.”; “v = s / temperature - math.log(-math.log(u))”; “return math.erfc(z / math.sqrt(2))”; “Linux glibc can differ from Apple libm”
- **What is weak:** all randomness goes through SplitMix64 and `derive_seed` (SHA-256 of the seed path), with explicit iteration orders and sorted JSON keys, and agent decision n uses `derive_seed(seed, agent_name, "decision", n)`. Replay compatibility requires bit-exact reproduction, and every new mechanism above (root-parallel search, belief weighting, transpositions) must keep it. The PRNG is portable, but AI decisions call the platform C math library:
  - Gumbel sampling in rollouts uses `log(-log(u))` (`policy.py:98`).
  - Priors and the logistic leaf use `exp` (`mcts.py:314`, `evaluation.py:303-307`).
  - UCB uses `log` (`mcts.py:297`).
  - The paired variances use `**`, which is libm `pow` (`mcts.py:199`, `setup.py:109`).
  - Report p-values use `erfc` (`stats.py:60`).

  Apple libm and glibc can return different last bits. On the port's macOS machine, `x ** 2` differed from `x * x` for 316 of 284,674 inputs (inventory 09 §5.2). A one-ulp difference can flip a near-tie, so Python itself can play different games, and write a different `results.json`, on Linux and macOS. SPEC's "platform-independent" claim holds only for the PRNG, and no golden platform is named. Earlier drafts of this entry and of W-35 assumed cross-platform bit-identity.
- **Resolution plan:** Rust keeps the contract. `gcg-core` provides the only RNG and seed derivation (already verified against Python vectors). A lint test forbids `std::time`, `rand`, `HashMap` iteration and `std::hash` in the engine, AI and report crates (ordered maps or stable hashes only). Every parallel mechanism merges its results in index order. For the reference, the golden platform is named: the platform that produced the Python vectors and traces (macOS arm64, Python 3.15, Apple libm). On that platform the reference port reaches Python's libm results in safe code for three of the four functions, as inventory 09 verified (§5.2 and §5.3, 0 mismatches): `f64::exp` and `f64::ln`, which lower to the same libm calls, and `powf(std::hint::black_box(2.0))` for the squares. `erfc` is the exception. `f64::erfc` is unstable in Rust 1.99, so the only bit-exact option found is an `extern "C"` call to libm `erfc`, and that needs `unsafe`, which the workspace denies (`unsafe_code = "deny"`, `Cargo.toml:18`). This needs the lead's decision (inventory 09, open question 2), between two options:
  - (a) allow `unsafe` in one module of the report crate, which holds only the `erfc` declaration and a safe wrapper;
  - (b) keep the deny and compute the reference p-values with the portable `erfc` below.

  Option (b) matches Python only where both results round to the same 6 digits. It is therefore checked on every p-value of the equivalence corpus, and a mismatch there means option (a) is needed. Until the lead decides, the reference p-values are not claimed to be byte-identical. On other platforms the reference is only as deterministic as their libm, and the Rust docs say so.

  The strengthened search and every gcg-eval statistic need bit-identity on every platform, not correct rounding. The only `**` sites are squares (`mcts.py:199`, `setup.py:109`), and the strengthened code computes them as `d * d`: IEEE multiplication is correctly rounded on every platform, so no portable `pow` is needed. `exp`, `ln` and `erfc` are written in safe Rust in `gcg-core`, with no new dependency: `Cargo.lock` has no math crate, and the `libm` crate is neither available offline nor verified (inventory 09). The functions port the classic fdlibm algorithms (argument reduction, then a fixed polynomial). They use only IEEE-754 basic operations (+, −, ×, ÷, sqrt), with no fused multiply-add and no platform call. Those operations are correctly rounded on every IEEE-754 platform, so the results are bit-identical everywhere. The accuracy target is an error below 1 ulp. `analysis.json` uses the portable `erfc`.
- **Verification** (unit + e2e):
  - The lint test.
  - Identical outputs for 1, 4 and 10 workers.
  - Reference equivalence with Python on the golden platform, on the equivalence seeds. With option (b), the test also lists every reference p-value whose 6-digit rounding differs from Python's (the expected count is 0).
  - Strengthened presets and gcg-eval outputs bit-identical on macOS arm64 and Linux x86-64 (CI matrix), on the equivalence seeds. Any `results.json` p-value that differs between the two platforms is listed.
  - A unit test checks the portable `exp`, `ln` and `erfc` against reference values from `port/oracle/export_vectors.py`. The oracle computes them with Python's standard `decimal` module, available offline. Its `exp` and `ln` are correctly rounded. `erfc` comes from the series for erf, evaluated with enough digits to cover the series' cancellation, and each value is confirmed by recomputing it at twice the precision. The inputs are recorded arguments from the equivalence runs plus random inputs. Every error must be below 1 ulp, and the CI matrix must give identical bit patterns.
- **Status:** open: the Linux x86-64 run of the cross-platform check has not happened; it is the `portable-math-linux` job of `.github/workflows/rust.yml`, which runs once the user approves a push. `portable_math` is not adopted in the presets (lead decision, `docs/port/PLAN.md` §0.1, end of Phase 6). The `portable_math` mechanism (`crates/gcg-search/src/mech/p6o/math.rs`) routes the strengthened search's math through the pure-Rust `libm` crate. Also in place: the lint, the named golden platform (A1), `erfc` through the FFI of `crates/gcg-core/src/libm.rs` (lead decision A2), and identical outputs for any worker count. Of the three items left at 1007ca9 (`docs/port/eval/weakness-status/GAP-A2.md`):
  1. Done. `portable_math` now reaches every float path of the strengthened search, including the shared `gcg-ai` code it calls: `gcg_ai::FloatMath::{Platform, Portable}` (`crates/gcg-ai/src/math.rs`) and the `_with` variants of `logistic`, `evaluate`, `pick`, `choose`, `play_to_end`, `first_player_choice` and `without_effect`. The frozen entry points call their variants at `Platform`, so the reference is unchanged.
  2. Open. The check is now one test, `crates/gcg-search/tests/scenarios/gapa2_portable_math.rs`: 46 recorded SHA-256 keys of portable searches and pre-game choices, equal on every IEEE-754 platform by construction. It passes on macOS arm64. The Linux x86-64 run has not happened: `.github/workflows/rust.yml` has one job, on `macos-15`, and no push is recorded (A29, M17). T-133 names only that macOS runner, and PLAN §12 calls the Linux matrix job optional (`docs/port/PLAN.md:3027-3029`).
  3. Measured and decided: not adopted. Every row is null: no winner changed in 1,200 games against the controls. Adopting it after the final evaluation would change the evaluated presets' bits, so M15b left it off (`crates/gcg-search/src/presets.rs` `SHIPPED_MECHANISMS`), so the shipped presets still take `exp` and `ln` from the platform library, and their bit-identity across platforms is not claimed. The 8x `standard` leave-one-out row is not measured.
- **Evidence:**
  - `mech::p6o::math::tests::platform_is_the_reference_and_portable_is_the_libm_crate`.
  - `portable_exp_and_ln_are_within_one_ulp_of_the_golden_platform`: 8,000 recorded inputs, all within one ulp, 7,358 bit-identical.
  - `mech::tests::portable_math_runs_the_same_search_with_portable_functions`.
  - `crates/gcg-core/tests/vectors.rs::libm_parity_matches`.
  - `crates/gcg-cli/tests/benchmark.rs::needs_engine_outputs_are_identical_for_any_worker_count`.
  - Item 1 (GAP-A2, pass): `crates/gcg-ai/src/math.rs` tests `platform_is_gcg_core_libm_and_portable_is_the_libm_crate`, `the_providers_agree_within_one_ulp` and the lint `every_platform_libm_call_of_the_crate_is_in_this_module`; `crates/gcg-search/src/mech/gapa2/mod.rs` tests `the_search_policy_move_is_gcg_ai_choose_with_its_library`, `the_reference_play_draw_choice_takes_the_configuration_math` and the lint `every_float_path_of_the_search_goes_through_its_math`, with negative controls. The tier-1 corpus stays 3804/3804 and the null-configuration tests pass.
  - Item 2: `gapa2_portable_math.rs::portable_math_searches_give_the_recorded_bits_on_every_platform` and `the_recorded_bits_depend_on_the_library` (with the platform library 1 of the 46 keys differs). M15b re-recorded the keys for the new presets (commit 8c89b5e).
- **Ablation:** no AI effect for the shipped presets, which have `portable_math` off (the null-configuration tests). Rows (GAP-A2, `docs/port/eval/GAP-A2/rows/`, 400 games each):
  - Individual (dev 22721400): 202/400 (0.505, [0.456, 0.554]), +0.010 [-0.054, +0.074], discordant 23/21, 9925 vs 9943 / 119782 vs 120899. Its null control on the same master gives the same counts; 3 games differ in effort only, 0 in winner.
  - Leave-one-out from the M15 `fast` (dev 22721500): 214/400 (0.535, [0.486, 0.583]), +0.070 [+0.004, +0.135], 30/16, 14819 vs 14900 / 278932 vs 280563. Its placebo on the same master gives the same result, and game by game no winner differs, so the difference is the protocol's placebo noise.
  - Leave-one-out at high budget (dev 22721600, the M15 shipped mechanisms at 256 / 640 / 2560): 184/400 (0.460, [0.412, 0.509]), -0.080 [-0.154, -0.005], 22/38, 32021 vs 32498 / 590975 vs 601004. Its placebo gives the same result, and no winner differs.

### W-138 — Multi-card choices are sequential single picks
- **Category:** design-decision · **Resolution type:** port-preserve + AI (strengthened search)
- **Sources:** `docs/ARCHITECTURE.md:53-56`, `docs/INTERFACES.md:21-28`, `src/gcg_sim/ai/actions.py:41-45`
- **Quotes:** “Multi-card choices are sequential single picks with a DONE option”
- **What is weak:** picks are ordered so each subset is reachable once; this shapes the search's branching factor (one node per partial subset) and the replay action encoding (`Action(kind, a, b, c)` with the EX count packed into `c` modulo `ALT_PLAY_BASE`).
- **Resolution plan:** the engine and replay encoding stay exactly as in Python. The strengthened search may treat a sequence of single picks of one choice as one macro-move (the chosen subset) at its own nodes, expanded back into the engine's single picks when applied, so subsets share statistics without changing the engine interface.
- **Verification** (unit + ablation): replays recorded by the strengthened search are valid sequences of single picks that the reference engine accepts; ablation row for macro-moves.
- **Status:** resolved by measurement (not adopted): `macro_multi_pick` expands a SELECT of 2-3 cards into its complete subsets, at most 20, each one tree move. It measured null (+0.010; multi-card choices are rare on the panel). It was in `B_mid`, which is null against `B_core`, and it is off in both presets. The reference keeps sequential single picks, and the engine and replay encoding are unchanged.
- **Evidence:** both tests pass:
  - `p6d_scenarios::p6d_macros::subset_moves_replay_as_single_picks`: 10 fuzz-deck games. Every recorded action replays with `apply(check=true)` to the same winner and digest. 3 searches used subset moves, and 4 multi-card picks were played.
  - `mech::p6d::macros::tests::subset_keys_are_distinct_and_never_single_pick_keys`.
- **Ablation:**
  - Individual (dev 24600001): 202/400 (0.505, [0.456, 0.554]), +0.010 [-0.054, +0.074], discordant 23/21, 10039.9 vs 9976.0 / 124246.4 vs 121542.7 (`docs/port/eval/P6-D/individual-macro_multi_pick-true.json`).
  - Bundle: in `B_mid`.

## Later additions

Two completeness reviews found these entries after the first numbering: W-139 to W-154, then
W-155 to W-164. Each keeps its own category, and the IDs continue the sequence, so earlier IDs
stay stable.

### W-139 — Opponent nodes are tuned against the searcher's true private cards
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/mcts.py:1-12`, `src/gcg_sim/engine/observe.py:56-83`, `src/gcg_sim/ai/evaluation.py:178-188`, `docs/python/AI.md:462-469`
- **Quotes:** “One tree is built from the deciding player's point of view.”; “Return a full game state consistent with ``observer``'s information set.”
- **What is weak:** `determinize` redraws only the cards the observer cannot identify, so the searcher's own hand, and every card it has seen, is the same in every world. Three things are computed against the searcher's real cards: the opponent nodes' statistics, the opponent's rollout choices, and the leaf evaluation (whose hand counts and castable-removal "answers" read both hands). In the tree, the opponent effectively plays around tricks the searcher really holds and is never bluffed. The search therefore undervalues holding a trick, and any line that relies on the opponent not knowing. This is inferred from the design and has not been measured. W-05 covers the other direction: opponent nodes pooling statistics over the opponent's own hidden cards.
- **Resolution plan:** strengthened search, partial multiple-observer ISMCTS. A fixed share of each search's iterations (chosen by ablation, starting at 1 in 4, counted in the node budget) is played in "opponent-view" worlds. In those worlds, the searcher's private cards (its hand and every other card the opponent cannot identify) are redrawn from the opponent's information set. Opponent nodes select moves by the statistics of those iterations only, falling back to all iterations below a minimum visit count. The searcher's own nodes keep the normal iterations. This combines with W-05's buckets and W-07's belief weights. The reference is unchanged.
- **Verification** (scenario + ablation): scenario "held trick". The searcher holds an action-step AP−3 Command and the Resources to cast it, and the opponent can attack the searcher's Unit. With the trick, the Unit survives and wins that battle. The ground truth is fixed by 2,000 paired simulations against a reference `standard` opponent that plays from its own information set: keeping the Resources open beats spending them in the main phase. Over 16 seeds, the reference keeps the trick in at most 8 (recorded; its opponent nodes learn not to attack, so holding looks worthless), and strengthened `standard` keeps it in at least 12. Ablation row alone and leave-one-out.
- **Status:** resolved by measurement (not adopted): `opponent_view_share` now builds opponent-view worlds at every decision, 32 of 32 vector decisions (P6-C: 11), on W-08's engine identity API (`crates/gcg-search/src/mech/p6c/view.rs`; `docs/port/eval/weakness-status/GAP-B.md`). The cards the root options name keep their identities, so every root move plays the card it names, and everything else hidden from the opponent is redrawn from the opponent's view with exact pools. Where no option names a hidden card, the redraw is P6-C's bit for bit. It measured null alone and negative at the edge at high budget, so it is off in both presets. The scenario bar (the reference keeps the trick in at most 8 of 16, strengthened `standard` in at least 12) is met by the shipped `standard` without the mechanism, and the mechanism alone does not move the null configuration, so the bar does not isolate it. Residue: in the opponent-view worlds the opponent's nodes still see the named cards.
- **Evidence:** these tests pass:
  - Unit test `mech::p6c::tests::w139_opponent_view_worlds_hide_the_held_trick`: of 400 worlds, the 100 opponent-view ones hold the trick in 2.
  - `an_opponent_view_iteration_trains_only_the_opponent`, `opponent_view_worlds_need_root_options_the_opponent_can_identify` and `view::tests::view_worlds_follow_the_share`.
  - `mech::p6c::gapb_tests::w139_opponent_view_worlds_keep_the_named_cards_at_every_decision`: 50 opponent-view worlds of 200 keep the playable card's identity, the unnamed trick is redrawn, and the opponent's information set is the usual world's.
  - Scenario test W-139 (T-123; `crates/gcg-search/tests/scenarios/p6c_w139_held_trick.rs::w139_held_trick_decisions_are_recorded`), which passes as a record: reference 0/16, share 0.25 0/16, share 0.5 0/16.
  - `mech::p6c::gapb_tests::w139_held_trick_decisions_at_every_decision_are_recorded` asserts the bars on the binding position and on a variant with a castable Unit in hand (one named hidden card). Run at HEAD `a926ac7` (M15b presets, 2026-11-04, `cargo test --release -p gcg-search --lib w139_held_trick_decisions_at_every_decision_are_recorded -- --nocapture`), keeps of 16: without the Unit, null 0, share 0.25 0, share 0.5 0, shipped `standard` 16, shipped `standard` + 0.25 10; with the Unit, 0, 0, 0, 16 and 8. GAP-B recorded 16 and 15 for shipped `standard` + 0.25 on the M15 presets.
- **Ablation:** iterations and applies per game are challenger / opponent.
  - P6-C individual (dev 20300001), `opponent_view_share=0.25`: 200/400 (0.500, [0.451, 0.549]), +0.000 [-0.073, +0.073], discordant 28/28, 9952.2 / 9911.3, 120460.5 / 118543.1.
  - P6-C high budget (dev 20300002): 200/400 (0.500, [0.451, 0.549]), +0.000 [-0.071, +0.071], 27/27, 32392.4 / 32598.3, 382912.1 / 383899.0.
  - The three P6-C flags together: 193/400 (0.482, [0.434, 0.531]), -0.035 [-0.104, +0.035], 22/29 (`docs/port/eval/P6-C/`).
  - At every decision (GAP-B), individual (dev 22800001), 0.25: 205/400 (0.512, [0.464, 0.561]), +0.025 [-0.043, +0.093], discordant 27/22, 9983 / 9957, 122091 / 121324 (`docs/port/eval/GAP-B/individual-opponent_view_share-0_25.json`).
  - At every decision, high budget (dev 22800002, the M15 shipped mechanisms at 256 / 640 / 2560, with 0.25 against without): 186/400 (0.465, [0.417, 0.514]), -0.070 [-0.139, +0.000], 19/33, 32527 / 32491, 605086 / 599816 (`high-budget-opponent_view_share-0_25.json`).
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-140 — In-tree priors come from the first world that reaches each node
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/mcts.py:128-130`, `src/gcg_sim/ai/mcts.py:281-282`, `src/gcg_sim/ai/mcts.py:305-317`, `src/gcg_sim/ai/policy.py:133-156`, `docs/python/AI.md:97-100`
- **Quotes:** “_add_children(root, first_world, dec, root_moves, config)”; “node.children.setdefault(_child_key(dec, key, cfg), _Child(dec.player, wgt / total))”; “Priors are the playout policy's scores through a softmax (temperature 1.5).”
- **What is weak:** a node's priors are the softmax of `policy.option_scores`, computed in one world: the root's in world 0, and each in-tree node's in the world of the iteration that first adds its children. When a later world offers a move the node has not seen, `_add_children` scores that world's whole move set, but `setdefault` keeps the earlier priors. A node's priors then mix softmaxes normalised over different move sets. At opponent nodes the moves themselves (the cards of a sampled hand) depend on the world, so one sampled hand can set the priors. At the root the moves are the true decision's, and the MAIN scorer's board summary reads only public state (`policy.py:133-156`), so root priors probably do not depend on the world. However, no test checks that every scorer reads only the decider's information. The priors stay honest about information, but they are noisy. No existing entry covers this.
- **Resolution plan:** strengthened search: an in-tree node's prior for each move averages the policy scores over the first K worlds that reach the node (K = 4, the common-random-number worlds). The priors are normalised over the union of moves seen, and recomputed when a new move appears. The root prior is computed once. The reference keeps first-world priors.
- **Verification** (unit + ablation): a unit test shows that a strengthened node's priors are the same whatever order the same K worlds arrive in. A second unit test, on 1,000 recorded positions, checks that root priors are equal across 8 determinizations for every decision kind. The reference result is recorded, and any difference shows which scorer reads hidden state. Ablation row alone and leave-one-out.
- **Status:** resolved by measurement (not adopted): `prior_worlds{K}` averages each in-tree node's priors over the first K distinct worlds. It measured -0.040 at K = 4 and is off in both presets.
- **Evidence:** both tests pass:
  - `mech::p6d::priors::tests::priors_do_not_depend_on_the_order_the_same_worlds_arrive_in`: 24 vector decisions, three orders, identical bits.
  - `p6d_scenarios::p6d_priors::root_priors_are_equal_across_eight_determinisations`: 1,000 positions over every decision kind met. No scorer reads hidden state at the root; this is the reference result, recorded.
- **Ablation:**
  - Individual (dev 24600001): 192/400 (0.480, [0.431, 0.529]), -0.040 [-0.101, +0.021], discordant 16/24, 9947.6 vs 9859.0 / 119954.8 vs 118215.2 (`docs/port/eval/P6-D/individual-prior_worlds-4.json`).
  - Leave-one-out: not applicable (not in the shipped bundle).

### W-141 — The no-effect branch check compares a branch with a policy line that may replay the move
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/noop.py:13-17`, `src/gcg_sim/ai/noop.py:154-165`, `src/gcg_sim/ai/noop.py:197-210`, `src/gcg_sim/ai/config.py:68`, `docs/port/inventory/07-ai-search.md:1071-1073`
- **Quotes:** “idled = lines.world.clone()”; “which may replay the move under test”
- **What is weak:** `noop_branch_choices` is on in the presets. With it, a move judged without effect is re-checked once per option of its first choice, and kept when some branch "beats idling". When a branch's replay diverges, the idle baseline becomes the deterministic policy continued from the world without the move (`noop.py:203-205`). That policy still holds the card and may cast the same Command or activate the same ability. The comparison is then "this branch against the policy's own line", not "the move against idling". A useful branch that is no better than the policy's own use of the move does not count, so the move can be pruned. The port inventory flags this as an open question, and W-12 does not cover it.
- **Resolution plan:** the reference keeps it bit-exact. The strengthened no-effect check builds the idle line with the move under test removed from the options of every decision in the continuation. "The move" covers every variant of the same card's play or activation. The baseline is then real idling.
- **Verification** (unit + ablation): a testkit position where the deterministic policy, continued without the move, casts the same Command. The reference prunes the move (pinned). The strengthened check keeps it, and its idle line contains no action of that card. Included in the bundle's leave-one-out row with W-12.
- **Status:** resolved by measurement (not adopted): `real_idle_line` takes every policy move of an idle line from options without any play or activation of the move's card. It measured null. It was in `B_mid` with W-12's mechanisms, which is the verification's "bundle's leave-one-out row with W-12", and it is off in both presets. The reference keeps its policy line.
- **Evidence:** unit test `mech::p6e::tests::the_real_idle_line_never_replays_the_move_under_test` passes. It uses game 53 of greedy self-play, decision 24. The reference's check prunes the Command (pinned), and its idle line casts it. The strengthened check keeps it, and its idle line takes no action of the card.
- **Ablation:**
  - Individual (dev 20005000): 197/400 [0.444, 0.541], -0.015 [-0.080, 0.050], discordant 21/24, 9823.9 vs 9817.4 / 118459.2 vs 117403.8, 13 games changed, +0 -0 (`docs/port/eval/P6-E/individual-real_idle_line.json`).
  - All seven P6-E flags (critic v2): 199/400 [0.449, 0.546], -0.005 [-0.072, 0.062], 23/24, 129 games changed, +4 -2 (`individual-p6e_all.json`).
  - Bundle: in `B_mid`.

### W-142 — The evaluation is fitted on true-state features but scores determinized states; Burst density is computed once per search
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `src/gcg_sim/ai/tuning.py:148-157`, `src/gcg_sim/ai/mcts.py:113-120`, `src/gcg_sim/ai/mcts.py:263`, `src/gcg_sim/ai/evaluation.py:132-140`, `src/gcg_sim/ai/evaluation.py:178-188`, `docs/python/AI.md:167-169`, `docs/port/inventory/08-ai-policy-eval.md:800-806`
- **Quotes:** “density = burst_density(st, st.active)”; “states only, so the opponent's hand composition is a sample from the player's information”; “in both cases with `density = burst_density(st, st.active)` on the TRUE state.”
- **What is weak:** tuning records features at the first MAIN decision of each turn, on the true game state. That includes both players' real hands (hand counts by card type, and castable-removal "answers"), with Burst density from the active player's view. The search scores determinized worlds instead, where the opponent's hand is a uniform sample from its unseen cards (W-07), not the cards that player chose to keep. So the weights are fitted on one feature distribution and used on another. In addition, the density used at every leaf is computed once, at the root, from the searcher's view (`mcts.py:117`). It is reused even when the leaf is two turns later, when its turn player differs, or when Shields and unseen cards have changed. Neither effect has been measured, and W-34 to W-40 do not mention them.
- **Resolution plan:** the gcg-eval `tune` tool (W-34, W-40) records each position's features in a determinization from the recorded player's view. It uses the same feature and density functions the search calls (one shared code path). It reports log-loss for true-state and determinized recording side by side, and the strengthened presets use whichever fit is better out of fold. The strengthened leaf recomputes Burst density from the searcher's known cards in the leaf world. The reference keeps both behaviours.
- **Verification** (unit + ablation): a unit test shows that, on 100 positions, the tuning tool's determinized features equal the search's leaf features for the same world and seed. Log-loss table for both recording modes (out of fold by game, and on the held-out panel decks). Ablation rows for the determinized fit and for per-leaf density, alone and leave-one-out.
- **Status:** resolved by measurement (not adopted): two flags, both measured and neither adopted.
  - `determinized_fit` fits the determinised samples. It measured negative.
  - `leaf_density` recomputes the Burst density at every leaf. It measured null, was in `B_mid`, and stays available.

  The recorder and the leaf share one code path. The verification's log-loss table "on the held-out panel decks" is not produced, by the decision recorded in `docs/port/eval/weakness-status/GAP-A2.md`, as for W-36: the steering roles refuse held-out pairs, and the gate ("the strengthened presets use whichever fit is better out of fold") decides nothing while neither flag ships and the presets use `ROLE_WEIGHTS`. The fits' held-out pair, purple-blue-swarm : red-purple-control, is a pair of tuning decks. If a fit is ever proposed for adoption, the held-out scoring of W-36 comes first.
- **Evidence:**
  - Unit test `mech::p6g::tests::w142_recorded_features_are_the_leaf_features` passes: 100 positions, both observers, bit for bit.
  - Log-loss on determinised samples, the determinised fit against the true-state fit: out of fold 0.5318 against 0.5338, on the held-out pair 0.5740 against 0.5768, and on the unseen `fast` positions 0.5373 against 0.5413 (`docs/port/eval/P6-G/fit-round1.json`, `fit-standard-selfplay-test-fast.json`).
- **Ablation:**
  - `determinized_fit`:
    - individual (dev 20700100): 194/400 (0.485, [0.436, 0.534]), -0.030 [-0.106, +0.047], discordant 28/34, 9583 vs 9709 / 116655 vs 116307.
    - high budget (dev 20700200): 189/400 (0.472, [0.424, 0.521]), -0.055 [-0.125, +0.016], 21/32.
  - `leaf_density`:
    - individual: 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.037, +0.107], 31/24, 9806 vs 9854 / 119302 vs 119835.
    - confirmation (confirm 30700100): 802/1600 (0.501, [0.477, 0.526]), +0.003 [-0.033, +0.038], 105/103.
    - high budget: 199/400 (0.497, [0.449, 0.546]), -0.005 [-0.074, +0.064], 25/26.
  - Bundle: `leaf_density` was in `B_mid`.

### W-143 — Battle-damage redirect cycles recurse without bound and crash the game (two GD04-095)
- **Category:** engine · **Resolution type:** Rust fix (error path only) + data tripwire
- **Sources:** `src/gcg_sim/engine/core.py:576-599`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:126-137`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:153-163`, `src/gcg_sim/runner/game.py:130-136`, `src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md:905-909`, `docs/port/inventory/06a-engine-core-interp.md:1189-1190`
- **Quotes:** “return resolve_damage(st, dest, amount, source=source, battle=battle, by=by)”; “d.Apply(d.Var("t1"), redirect, Duration.THIS_TURN, aux=d.This()),”; “has no cycle guard”
- **What is weak:** `resolve_damage` follows a REDIRECT_BATTLE_DAMAGE rule by recursing on the destination, and keeps no record of the Units it has visited. GD04-095 Lunamaria Hawke makes a cycle reachable. Its 【When Linked】 text reads: "Choose 1 of your (Minerva Squad) Units. During this turn, battle damage it would receive is dealt to this Unit instead." Pair one copy onto Minerva Squad Unit A and choose B. Pair a second copy onto B and choose A. Then attack with A. Reproduced this session with the testkit (two GD04-062 and two GD04-095): `RecursionError: maximum recursion depth exceeded`. The runner wraps the error in a GameFailure, which aborts the whole benchmark with no reports (W-105). A search that reaches such a position in a rollout fails the same way. Rule 10-1-9 defines substitution effects but says nothing about chains or loops. The port inventory had marked the recursion "Not known to be reachable", and W-47 covers loops inside `advance()` only.
- **Resolution plan:** the Rust `resolve_damage` keeps the set of Units visited along the redirect chain, and returns `EngineError::RedirectCycle` (naming them) when a destination repeats. The runner treats the error as Python treats the crash: the game fails, and the run aborts with no reports and exit code 1. Outputs therefore stay equivalent for every game that does not fail. Only the error text differs, as for other exception texts (inventory 01, open question 6). The rules question (does each substitution apply at most once per damage event?) goes to the Python data workflow as a curated conflict. Only its resolution may change the behaviour, and then in both implementations together. A data-gate tripwire lists every card whose IR grants REDIRECT_BATTLE_DAMAGE (today GD04-087 and GD04-095), and fails when the list changes, so every new redirect source is reviewed for cycles.
- **Verification** (unit + e2e + negative control):
  - Unit: the two-Lunamaria testkit position returns `RedirectCycle` in Rust (the Python `RecursionError` is recorded).
  - Unit: a redirect chain without a cycle (A to B to C) lands on the same Unit in both implementations.
  - The tripwire passes on the packaged pool. As a negative control, a synthetic third redirect card in a temporary data copy fails it.
  - e2e: a benchmark whose test agents build the cycle exits 1 with no reports, as Python does.
- **Status:** resolved: `core::follow_redirects` returns `EngineError::RedirectCycle` where Python recurses until `RecursionError`. The runner fails the game: exit 1, no reports, as in Python, with only the message different (`docs/port/EQUIVALENCE.md`, D2). The strengthened search scores such a simulated line with `engine_error_leaf` (P6-O), which is on in both shipped presets. The rules question stays with the Python conflict workflow.
- **Evidence:**
  - Unit tests: `crates/gcg-engine/tests/p6j_engine.rs::w143_two_lunamarias_build_a_cycle_that_fails_the_game`, `w143_a_redirect_chain_lands_on_its_last_unit` and `core::tests::redirect_chains_land_and_cycles_fail`.
  - Tripwire: `crates/gcg-effects/tests/p6j_ir_tripwires.rs::w143_redirect_sources_are_reviewed` (exactly GD04-087 and GD04-095), with the negative control `w143_a_third_redirect_source_fails_the_tripwire`.
  - e2e: `crates/gcg-diff/tests/p6j_runs.rs::w143_a_benchmark_that_builds_the_cycle_fails_without_reports`. A mirrored Minerva Squad deck at seed 4 fails in match 46, game 1, with no report written; Python exits 1 the same way.
  - `crates/gcg-search/tests/engine_error_leaf.rs` covers fuzz game 221:
    - at `reference_mapping(Fast)`, the configuration it failed with, it completes with error leaves;
    - with the flag off it fails with `RedirectCycle [36, 3, 36]`;
    - the shipped `fast` finishes the game without reaching the cycle.
- **Ablation:**
  - Engine change: no AI effect (row J). Error path only.
  - `engine_error_leaf` leave-one-out (dev 21500200): identical to the placebo game for game, 0 of 400 changed. 207/400 (0.517, [0.469, 0.566]), +0.035 [-0.032, +0.101], discordant 27/20, 32778 vs 32388 / 598554 vs 579017 (`docs/port/eval/M15/rows/dev-loo-engine_error_leaf-400.json`).
  - With the flag on, the null configuration replays all 661 searched decisions of the tier-1 `search` set with 0 error leaves (`null_config_search_set.rs`).

### W-144 — The deck parser accepts unbounded counts: a huge count crashes with a traceback or allocates without limit
- **Category:** validator · **Resolution type:** Rust fix (diagnostics)
- **Sources:** `src/gcg_sim/deck/parse.py:18`, `src/gcg_sim/deck/parse.py:53-56`, `src/gcg_sim/deck/parse.py:89`, `src/gcg_sim/deck/parse.py:101`, `src/gcg_sim/cli.py:102-110`, `docs/port/inventory/03-data-deck.md:840-844`
- **Quotes:** “count = int(m["count"])”; “target.extend([e.card_number] * e.count)”; “that escapes `cli._read_deck` (not caught) → traceback.”
- **What is weak:** `\d+` accepts a count of any length (and any Unicode digit, inventory H4). A count with more than 4,300 digits makes `int()` raise ValueError. `_read_deck` catches only OSError, DeckError and UnicodeDecodeError, so `gcg-sim validate` prints a traceback. Reproduced this session with a 5,000-digit count: `ValueError: Exceeds the limit (4300 digits) for integer string conversion`. A smaller but still huge count passes parsing and reaches `build_deck`'s `[e.card_number] * e.count` before any size check, so memory use is unbounded (MemoryError or a hang according to the inventory; not run). Deck files are user input, and W-77 covers format limits, not this.
- **Resolution plan:** the Rust parser keeps Python's grammar (Unicode digits included) but reads counts as `u64`. A count that does not fit is a SYNTAX violation naming the line. The deck model keeps per-entry counts and checks deck sizes and copy limits from them. It expands the card lists only after the deck has validated (at most 50 + 10 cards). For every count Python can handle, the violations are the same as Python's. Where Python crashes or exhausts memory, Rust reports a SYNTAX or size violation and exits 1.
- **Verification** (unit):
  - A 5,000-digit count gives a SYNTAX violation and exit code 1.
  - A count of 10^12 gives the main-deck size violation without allocating per copy (checked under a memory cap).
  - Over a generated corpus with counts up to 1,000, violation codes and messages are the same as Python's.
- **Status:** resolved (landed at M4, commit a4977be): counts are read as `u64` with Python's grammar. An over-long count is a SYNTAX violation that names the line. Decks are run-length, and `to_decklist` refuses totals above 65,535. P6-L adds the e2e and memory evidence.
- **Evidence:**
  - `deck.rs::tests::huge_counts_are_syntax_violations`.
  - `crates/gcg-data/tests/decks.rs::huge_counts_validate_without_expanding` (T-036).
  - `crates/gcg-data/tests/p6l_validator.rs::w144_huge_counts_in_files_and_the_extended_check`.
  - `validate_cmd::tests::w144_huge_count_exits_one_with_syntax`.
  - On the binaries: Python raises a traceback, while Rust `gcg-sim` and `gcg-eval` exit 1 with a SYNTAX message.
  - Memory: on a deck of 10^12 copies, Rust peaks at 45,268,992 bytes RSS, against 46,104,576 for the 50-card example. Python was killed (exit 137).
- **Ablation:** no AI effect (row L).

### W-145 — The packaged card data predates GD06 (released 2026-10-30), so its cards are unknown
- **Category:** data · **Resolution type:** Rust fix (data-driven loader) + reporting
- **Sources:** `src/gcg_sim/data/gcgapi/products.json:1`, `src/gcg_sim/data/gcgapi/manifest.json:3-5`, `README.md:130`, `src/gcg_sim/deck/parse.py:57-61`
- **Quotes:** “"set_code":"GD06","release_date":"2026-10-30"”; “"built_at": "2026-10-12T14:37:38.154Z",”; “unknown card id {written!r} (not in the card data)”
- **What is weak:** the packaged snapshot (gcg-api dataset 28, built 2026-10-12) lists two newer sets in `products.json`: GD06 Stardust Trails (released 2026-10-30) and GD07 Blazing Fist (due 2027-01-29). But `cards.ndjson` has no GD06 card number (0, counted this session). A deck with GD06 cards fails with UNKNOWN_ID, so benchmark decks from the post-GD06 meta cannot be modelled. Neither `data status` nor the reports say that a released set is missing, and no entry recorded how old the data is. Refreshing the data is a Python-workflow task (`/gcg-refresh-data`), and implementing the new cards is W-63's binding work.
- **Resolution plan:** no data change in the port. The Rust data crate loads a refreshed snapshot without code changes: sets, products and card numbers are data, never enums or constants. An opt-in `gcg-sim data status --freshness [--as-of DATE]` lists every product released after the snapshot's `built_at` that has no card numbers in the data. The flag is opt-in, so the default output stays equal to Python's. Running `/gcg-refresh-data` for GD06 is handed to the lead.
- **Verification** (unit + e2e): unit: a synthetic snapshot copy adds a new set (set code, product entry and ten vanilla card numbers). It loads, and a deck using the new cards validates, with no Rust code change. e2e: with the packaged data, `data status --freshness --as-of 2026-11-01` names GD06 as released on 2026-10-30 with 0 card numbers, and does not list GD07, which is not yet released.
- **Status:** resolved (`docs/port/eval/weakness-status/GAP-C.md`):
  - The loader is data-driven: a synthetic snapshot copy with ten new GD06 cards loads and resolves with no code change.
  - A deck of the new cards validates over a given `CardDb` through the public `gcg_data::legality::check` with `LegalitySources { db, .. }`, with no `gcg-data` change. A text parse over a given `CardDb` stays crate-private (`gcg_data::deck::parse_deck_with`); making it public is an interface proposal for the lead, outside the verification.
  - `gcg-eval data-status --freshness [--as-of DATE]` lists the products dated after the snapshot's `built_at` whose set has no card in the data. The tool reads no clock: without `--as-of` every later dated product is listed. With `--as-of 2026-11-01` it names GD06, released 2026-10-30, and not GD07.
  - Running `/gcg-refresh-data` for GD06 stays the lead's.
- **Option location (A30):** `gcg-eval data-status --freshness [--as-of DATE]`. The entry's plan named `gcg-sim data status --freshness`, and `gcg-sim` stays unchanged.
- **Evidence:**
  - `crates/gcg-data/tests/p6k_data_tripwires.rs::w145_new_sets_are_data_and_the_freshness_listing_names_gd06`.
  - `crates/gcg-eval/tests/gapc_tools.rs::w145_freshness_as_of_names_gd06_and_not_gd07` (pass): `--freshness --as-of 2026-11-01` prints `GD06 Stardust Trails[GD06]: released 2026-10-30, 0 card numbers` and no GD07; without `--as-of` GD07 is listed with its release date 2027-01-29; both usage errors exit 2.
  - `gapc_tools.rs::w145_a_synthetic_gd06_snapshot_loads_and_a_deck_of_its_cards_validates` (pass): a copy of `cards.ndjson` with ten vanilla GD06 Units loads, and a 50-card deck of them has no rules violation over it. Negative control: over the packaged database the same deck has exactly the ten GD06 numbers as UNKNOWN_ID.
  - `target/release/gcg-eval data-status --freshness --as-of 2026-11-01` (run 2026-11-04): "GD06 Stardust Trails[GD06]: released 2026-10-30, 0 card numbers", one product listed.
- **Ablation:** no AI effect (row K).

### W-146 — The rules markdown is checked against the official PDF only by its set of rule ids
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `.claude/skills/gcg-refresh-data/scripts/official_normalize.py:110-147`, `.claude/skills/gcg-refresh-data/SKILL.md:122-124`, `.claude/skills/gcg-refresh-data/SKILL.md:128-139`, `docs/research/OFFICIAL_SOURCES.md:219-221`, `data/official_raw/en_pdf_comprehensiverules_en.txt`
- **Quotes:** “`official_normalize.py check` (step 6) proves that the PDF and the markdown have the same”; “Update `src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md` from the PDF text.”; “Section 6 was compared word”
- **What is weak:** the markdown is written by hand from the PDF text whenever the rules change (step 2 of the new-version procedure). `check_rules` compares the version and date, the root copy's hash, the PDF hash and the sets of rule ids (566 in each), but never the rule text. Only section 6 was ever compared word for word. A transcription error in any other rule (a dropped "not", a wrong number or cross-reference) would pass every gate, even though traceability and the engine's interpretations cite that text. W-87 covers errors in the official file itself, and W-88 the omitted revision history; neither covers transcription fidelity.
- **Resolution plan:** a Rust data-gate test normalises both texts, then compares each rule's text in the markdown with the same rule in the cached PDF text extraction (`data/official_raw/en_pdf_comprehensiverules_en.txt`), listing every difference. Normalisation covers whitespace and line wrapping, the markdown escapes (`\<`, `\[`), the bold rule numbers and the `> Ex:` prefixes. Known PDF-extraction artefacts (ligatures, page headers and footers, hyphenation) go in an allowlist in the port's test data, with each entry naming its rule. The cached sources stay unedited. A real transcription error is handed to the lead for `/gcg-refresh-data`.
- **Verification** (unit + negative control): the test passes on the packaged files with a reviewed allowlist, whose size is reported. As a negative control, changing one word of one rule in a temporary copy of the markdown fails the test with that rule id.
- **Status:** resolved: each of the 566 rules is compared text against text between the markdown and the cached PDF extraction. One reviewed artefact is allowlisted: 10-3-3 "ta rgets".
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w146_rules_markdown_matches_the_pdf_rule_by_rule_with_negative_control`: 0 differences and an allowlist of size 1. Changing one word of 1-2-4 fails with exactly that rule.
- **Ablation:** no AI effect (row K).

### W-147 — The mulligan-outcome hypothesis is confounded by the AI's own redraw rule
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `src/gcg_sim/reports/hypotheses.py:151-164`, `src/gcg_sim/reports/aggregate.py:86-97`, `src/gcg_sim/ai/setup.py:63-83`
- **Quotes:** “Hypothesis: opening-hand consistency matters here; the early curve may need smoothing.”; “Redraw when the hand develops clearly less board than a fresh five from the rest of”
- **What is weak:** the redraw hypothesis compares the deck under test's win rate after a redraw with its win rate after keeping. Both groups are selected by the AI. It redraws only hands whose `opening_score` (Units and Pilots only, W-45) is below 90% of the average of fresh five-card samples, and a redrawn game starts from a fresh random hand. A gap therefore reflects the AI's threshold and scoring as much as the deck. Yet the statement suggests a deck-building cause ("the early curve may need smoothing"). W-90 covers the drawn-card and benchmark-card correlations, and W-94 the family size; neither covers this selection.
- **Resolution plan:** keep the reference hypothesis text byte-identical. `analysis.json` (written only with `--analysis`, W-90) adds:
  - the opening scores and the threshold of each kept or redrawn hand (already computed by `mulligan_choice`);
  - the opening score of the hand actually played (the kept hand, or the new hand after a redraw);
  - win rates by the played hand's opening-score quintile, with kept and redrawn games compared within each stratum.

  The comparison is labelled as a measure of the AI's mulligan rule, not of the deck.
- **Verification** (unit): on a synthetic game set where the win rate depends only on the played hand's opening score, the raw redraw gap is non-zero while the stratified difference's interval contains 0. The reference hypothesis is unchanged.
- **Status:** resolved: `analysis.json` `mulligan` gives, per game, the dealt and played hands' opening scores, the AI's threshold, the keep or redraw, the quintile win rates and a Mantel-Haenszel difference. It is labelled as a measure of the AI's mulligan rule. The reference hypothesis is unchanged.
- **Option location (A30):** `gcg-eval benchmark --analysis`.
- **Evidence:** `crates/gcg-runner/tests/p6m_analysis.rs::w147_the_redraw_gap_is_the_ai_rule_not_the_deck`: 4,000 synthetic games, a raw gap below -0.1, and a stratified interval that contains 0.
- **Ablation:** no AI effect (row M).

### W-148 — Saved replays are median-length games, and the CLI cannot replay other games.ndjson records
- **Category:** reporting · **Resolution type:** Rust fix (opt-in)
- **Sources:** `src/gcg_sim/reports/results.py:86-90`, `docs/REPORTS.md:116-118`, `.claude/skills/gcg-benchmark/SKILL.md:114-117`, `src/gcg_sim/cli.py:92-94`
- **Quotes:** “Representative games to save: wins and losses near their median length, plus a draw.”; “Check a few replays of losses for implausible plays before trusting a surprising”; “can be replayed the same way with”
- **What is weak:** `select_replays` saves `--replays` wins and losses nearest their median length, plus at most one draw. The benchmark skill tells users to check replays of losses for implausible plays. But the games that most need checking (very fast losses, surprising wins, games near the safety cap) are rarely saved. `gcg-sim replay` takes only replay JSON files, so any other `games.ndjson` record can be replayed only through the Python API (`gcg_sim.runner.replay(record, config)`).
- **Resolution plan:** additive, with the default outputs unchanged. `gcg-sim replay --games-ndjson FILE --results RESULTS --match M --game G` re-simulates any recorded game from the run's configuration, verifies it and prints its log. `benchmark --replays-extremes` also saves the shortest and longest win and loss, under distinct file names, and only when the flag is given.
- **Verification** (e2e): for every game of a small run, `replay --games-ndjson` matches the recorded outcome and final digest. Without the new flag, the replay directory and every report are byte-identical to Python's. With it, the extra files are the games of minimum and maximum length, with ties broken by match index.
- **Status:** resolved: `gcg-eval replay-game` re-simulates one `games.ndjson` record against the run's `results.json`. `--replays-extremes` also saves the shortest and longest win and loss. `results.json` and the representative replays are unchanged.
- **Option location (A30):** `gcg-eval replay-game --games-ndjson FILE --results RESULTS --match M --game G`, and `gcg-eval benchmark --replays-extremes`. The entry's plan named `gcg-sim replay --games-ndjson`.
- **Evidence:** `crates/gcg-eval/tests/p6m_cli.rs::w148_replay_game_verifies_every_game_of_a_run` and `crates/gcg-runner/tests/p6m_reports.rs::w148_extreme_replays_are_the_minimum_and_maximum_lengths`.
- **Ablation:** no AI effect (row M).

### W-149 — Which failure a parallel run reports depends on completion timing
- **Category:** runner · **Resolution type:** Rust fix (error path only)
- **Sources:** `src/gcg_sim/runner/run.py:45-53`, `src/gcg_sim/runner/run.py:67-80`, `src/gcg_sim/runner/run.py:88-106`, `docs/port/inventory/01-rng-runner-cli.md:1040-1042`
- **Quotes:** “In parallel runs, which failure gets reported when several matches fail depends on timing”; “done, pending = wait(pending, return_when=FIRST_COMPLETED)”
- **What is weak:** `_collect` waits for the next batch of completed matches, then calls `fut.result()` in index order within that batch. So the failure that gets raised, whose message names the match, game and seed, is the lowest failing index among the matches that happened to finish first. The serial path always reports the lowest failing match index. The error message, and so what a user investigates, depends on scheduling. That is an exception to the determinism contract, though on the error path only. W-105 and W-106 do not cover it.
- **Resolution plan:** on a failure at match k, the Rust runner cancels the matches above k (with W-106's cancellation flag) and lets the matches below k finish. It then reports the lowest failing index, which is the failure the serial path reports. Successful runs are unaffected.
- **Verification** (unit): a test agent that fails in matches 3 and 5 gives the match-3 error with 1, 2 and 8 workers, over 50 repetitions. With only match 5 failing, matches 0 to 4 finish before the error is reported.
- **Status:** resolved by construction (Phase 3): the pool reports the lowest failing match index and lets the lower matches finish.
- **Evidence:** `crates/gcg-runner/tests/p6m_reports.rs::w149_the_lowest_failing_match_is_reported` (50 repetitions at 1, 2 and 8 workers), `run.rs::the_lowest_failing_match_is_reported` and `a_lower_failure_supersedes_running_higher_matches` (T-074).
- **Ablation:** no AI effect (row M).

### W-150 — Determinism acceptance tests run only with the random test agent
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `tests/cli/conftest.py:31-33`, `tests/cli/test_console_script.py:64-72`, `tests/runner/test_pool.py:18-25`, `scripts/verify_wheel_offline.sh:34-38`, `docs/SPEC.md:52`, `docs/status.json:6`, `docs/port/inventory/09-python-hazards.md:869-873`
- **Quotes:** “return {**os.environ, AGENT_FACTORY_ENV: RANDOM_AGENT}”; “def test_results_do_not_depend_on_the_python_hash_seed(”; “Neither covers the”
- **What is weak:** criterion 2's tests use the random test agent. That covers worker-count identity (`test_pool.py`, `test_console_script.py`) and the PYTHONHASHSEED check, which is the only test that varies the hash seed. The real search AI's worker-count identity is checked only by `verify_wheel_offline.sh`: one pairing, 2 matches, 1 vs 2 workers, `standard`. No test runs MCTS under different hash seeds. The port inventory's own hash-salt runs covered greedy (4 games) and `fast` (1 game), not `standard` or `strong`. A hash-order dependence in AI code, such as iterating a set of strings, would pass the gate. W-118 notes the small offline check but not the hash-seed gap.
- **Resolution plan:** Rust e2e tests run the `fast` and `standard` presets (reference and strengthened) with 1, 4 and 10 workers on two pairings, and compare every deterministic output byte for byte. On the Python side, the equivalence corpus is generated twice, under PYTHONHASHSEED=1 and PYTHONHASHSEED=2, with all presets. The two runs must be byte-identical before Rust is compared against them, and a mismatch is a Python-side bug handed to the lead. Rust has no hash seed of its own to vary, because W-137's lint forbids `HashMap` iteration.
- **Verification** (e2e + negative control): the worker-count tests pass, and the two Python generations are byte-identical (recorded in the port's progress log). As a negative control, a test-only agent that breaks ties by iterating a std `HashMap` (randomly seeded) makes the comparison fail.
- **Status:** resolved: `crates/gcg-eval/tests/phase6_slow.rs::w150_every_preset_is_identical_at_1_4_and_10_workers` covers the reference `fast` and `standard`, the shipped strengthened `fast` and `standard`, and `head-to-head search:fast ref:standard`. Every output except `timing.json` is byte-identical at 1, 4 and 10 workers. The Python side (PYTHONHASHSEED 1 and 2) is recorded in `docs/port/progress.md` and `docs/port/EQUIVALENCE.md`. The slow test first ran on the pre-M15 presets. It passed with the M15 presets in GAP-D's clean-clone gate at 54c2bd2 (`phase6_slow` 4 passed; `docs/port/eval/weakness-status/GAP-D.md`). A run with the M15b presets is not recorded (`docs/port/eval/weakness-gaps.md`).
- **Evidence:** the test above, plus the negative control `w150_negative_control_hash_order_breaks_the_comparison`: a `HashMap`-ordered agent gives different action lists. T-003.
- **Ablation:** no AI effect (row I). Tests only.

### W-151 — Fuzz coverage counts cards in decks, not cards executed; tokens and EX cards are never in decks; the seed is fixed
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `src/gcg_sim/tools/fuzz.py:1-6`, `src/gcg_sim/tools/fuzz.py:33-43`, `src/gcg_sim/tools/fuzz.py:88-89`, `src/gcg_sim/tools/fuzz.py:132`, `src/gcg_sim/tools/fuzz.py:144-158`, `tests/slow/test_robustness.py:15-24`, `docs/SPEC.md:57`
- **Quotes:** “Generates decks that together include every implemented main-deck card”; “res.cards_seen.update(o["cards"])”; “res = run(GAMES, seed=2026, workers=workers)”
- **What is weak:** criterion 6 asks that the generated decks jointly include every card. The fuzz pool holds 956 coloured main-deck cards plus 108 Resources: 1,064 of the 1,152 card numbers (counted this session). The other 88 (29 UNIT TOKEN, 30 EX BASE, 29 EX RESOURCE) can never be in a deck. They appear only when an effect creates them, and nothing reports whether they did. `cards_seen` is deck membership, so a card that is never drawn, played or triggered still counts as covered. The 10,000-game test always uses seed 2026, so CI never explores new games. W-110 covers the agent choice and W-111 the tags; neither covers this.
- **Resolution plan:** Rust fuzz records, for each card number, whether its program ran (a step executed, a trigger fired or a static applied). Cards from decks and cards created by effects (tokens, EX Base, EX Resource) are recorded separately. The summary lists every pool card with a program that never ran, and each of the 88 effect-only numbers with its creation count. CI keeps the pinned seed 2026, for equivalence with Python's fuzz output. It adds a job whose seed is derived from the commit with `derive_seed` and printed, so any failure reproduces.
- **Verification** (unit + e2e): unit: a card that is in a deck but never drawn is reported as not executed. e2e: the 10,000-game run reports an execution status for every pool card, and every card with a program that never ran is listed with the tagged test that covers it. Each of the 88 effect-only numbers is either created in fuzz or covered by a golden record and a tagged test (W-117).
- **Status:** resolved: `scripts/fuzz_census.py` plays the seed-2026 fuzz games under the executed-program census. It records deck membership, executed programs and created instances per card number (`docs/port/eval/P6-N/fuzz-census-2026-10000.json`). `gcg-diff coverage --check` reports it.
- **Evidence:**
  - `uv run python scripts/fuzz_census.py --selftest`.
  - The census: 10,000 games, 0 failures, 1064 pool numbers, 787 whose programs ran.
  - Unit test `coverage::tests::w151_a_card_in_a_deck_whose_program_never_ran_is_reported_as_not_executed`.
- **Ablation:** no AI effect (row N).

### W-152 — Seat and first-player symmetry was checked once and has no regression test
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/PROGRESS.md:69-70`, `src/gcg_sim/runner/config.py:64-66`
- **Quotes:** “AI mirror 96 BO1 games, seat 0 won 50.”; “The deck under test sits in seat ``match_index % 2``, so seats alternate by match.”
- **What is weak:** seat symmetry was checked once, by hand. The check used 4,000 random-agent mirror games (first-player win rate 0.551 from seat 0, 0.539 from seat 1) and 96 AI mirror BO1 games (seat 0 won 50). No test in `tests/` checks it. Benchmark reports alternate the deck under test's seat by match, which would average away a seat-dependent engine bug (uid order, owner-ordered placement, the order of RNG draws).
- **Resolution plan:** a Rust e2e test plays mirror-deck games, with the same decklist in both seats, on fixed seeds: 4,000 random-agent games and 400 `fast` games. It asserts that the Wilson 95% interval of the seat-0 win rate contains 0.5. It also asserts that the first-player win rate does not differ by seat (two-proportion test, α = 0.01).
- **Verification** (e2e + negative control): the test passes on the Rust engine. As a negative control, a test-only engine build that gives seat 0 two extra Shields fails it.
- **Status:** resolved: `crates/gcg-eval/tests/phase6_slow.rs::w152_seats_and_first_player_are_symmetric` plays mirror-deck games. The seat-0 Wilson interval contains 0.5, and the first player's win rate does not differ by seat. The slow test first ran with the pre-M15 `search:fast`. With the M15 `search:fast` (GAP-D, at 54c2bd2) seat 0 won 198/400 [0.4463, 0.5438], and the first player won 120/206 from seat 0 and 116/194 from seat 1 (z = -0.313). A run with the M15b presets is not recorded (`docs/port/eval/weakness-gaps.md`).
- **Evidence:**
  - Random agent: seat 0 won 1982/4000 [0.4800, 0.5110], z = -0.535.
  - `search:fast`: seat 0 won 196/400 [0.4413, 0.5389], z = -0.510.
  - Negative control `w152_negative_control_two_extra_shields_fail_the_seat_check`: 2393/4000 [0.5830, 0.6133] fails.
- **Ablation:** no AI effect (row I). Tests only.

### W-153 — News and errata discovery in the refresh skill reads only a few listing pages and flags entries by title
- **Category:** tooling · **Resolution type:** tooling (Python side, handed to the lead)
- **Sources:** `.claude/skills/gcg-refresh-data/scripts/fetch_official.py:42-45`, `.claude/skills/gcg-refresh-data/scripts/fetch_official.py:139-149`, `.claude/skills/gcg-refresh-data/scripts/fetch_official.py:388-417`, `.claude/skills/gcg-refresh-data/scripts/fetch_official.py:429`, `.claude/skills/gcg-refresh-data/SKILL.md:80`, `.claude/skills/gcg-refresh-data/SKILL.md:98-104`, `data/sources.d/official.json`
- **Quotes:** “US news listing pages to read”; “errata|banned|restricted|rule|correction|revision|faq|comprehensive|regulation”; “Errata notices are usually filed under NEWS, not RULES.”
- **What is weak:** the `news` step finds new notices only on the listing pages it fetched. Those are the US all-news index (page 1 from the sources, pages 2 and 3 from `--news-pages 3`) and page 1 of the US and Asia RULES-tagged listings. It prints the uncached entries since the last retrieval date. It flags as RELEVANT only RULES-tagged entries and titles that match a fixed regex, while errata are usually filed under NEWS. If more than three pages of news accumulate between refreshes, the older entries are never listed, and nothing warns that the oldest entry read is newer than the cutoff. An erratum with an unusual title is printed unflagged among the other news. Errata that gcg-api lacks are found only through this list, which W-79 relies on. W-121 covers only testing and TLS.
- **Resolution plan:** the Python side is handed to the lead:
  - Page through the US and Asia all-news listings until an entry older than the cutoff appears, with a page cap that fails loudly.
  - Fetch every uncached entry since the cutoff, not only the flagged ones, and flag those whose text names a card number or says errata.
  - Add recorded-response tests for `parse_news` and the paging stop.

  On the Rust side, W-79's cross-check of recorded errata against the effective text is unchanged. The Rust gate summary also prints the newest `retrieved_at` of the news sources in `data/sources.d/official.json`, so an old news scan is visible.
- **Verification** (unit): a Python-side recorded-response test uses four fixture pages, where page 3 still ends after the cutoff. The stop rule must read page 4, and an exhausted page cap must be an error. On the Rust side, the gate summary's news retrieval date must equal the newest `retrieved_at` of the news sources.
- **Status:** resolved: `.claude/skills/gcg-refresh-data/scripts/fetch_official.py` pages through the US and Asia all-news listings to the cutoff, and an exhausted `--news-max-pages` is an error. It fetches every uncached entry since the cutoff, and flags entries by tag, title or text. Follow-up for the lead: the SKILL.md step 2 wording, with the replacement text in P6-N lead note 4 (T-142, M17).
- **Evidence:**
  - `uv run pytest port/oracle/tests/refresh_news -q`: 9 passed (T-155). Page 3 ends after the cutoff, so page 4 is read, and a cap of 3 is an error.
  - `crates/gcg-diff/tests/p6n_tooling.rs::w115_w153_*`: the gate line's date equals the newest `retrieved_at` (2026-10-15T20:03:21Z).
- **Ablation:** no AI effect (row N).

### W-154 — Untracked port artifacts would break the project's Python gates once committed
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `scripts/check_doc_commands.py:22-27`, `docs/SPEC.md:63`, `docs/port/inventory/11-python-profile.md:77-81`, `docs/port/inventory/10-tests.md:8-12`, `port/oracle/export_vectors.py:30`, `port/oracle/export_vectors.py:44`, `port/oracle/export_vectors.py:85`, `CLAUDE.md:10`, `scripts/verify_all.sh:15-21`
- **Quotes:** “"docs/**/*.md",”; “P=/private/tmp/claude-501/p0-prof”; “uv run ruff check . && uv run ruff format --check . && uv run mypy”
- **What is weak:** the port's files are still untracked (`docs/port/`, `port/`, `crates/`, `Cargo.toml`, `Cargo.lock` and two scripts). Two Python gates break once they are committed.
  - The documented-command gate (criterion 10, `check_doc_commands.py`) runs every bash block under `docs/**/*.md`, which includes `docs/port/`. The block in `inventory/11-python-profile.md` runs profiling scripts from `/private/tmp/claude-501/p0-prof` with a hard-coded checkout path, and a clean clone has neither. The block in `inventory/10-tests.md` adds pytest collection and a fuzz run to the gate.
  - `ruff check .` already fails in this working tree on `port/oracle/export_vectors.py` (PLR0124 at line 30, PIE808 at line 85), and `ruff format --check .` would reformat the file (both run this session). So the lint gate of CLAUDE.md and `verify_all.sh` fails as soon as `port/` is committed. `verify_all.sh` clones the committed HEAD, which is why it still passes.

  This file has no shell blocks.
- **Resolution plan:** before the first commit of port files:
  1. Mark the two inventory blocks `<!-- doc-check: skip <reason> -->`, or turn them into text blocks, since they record commands that were already run.
  2. Fix the two ruff findings and the formatting in `port/oracle` (preferred), or exclude the directory in ruff.
  3. Commit, then run `scripts/verify_all.sh` and `check_doc_commands.py` on the branch.

  Port docs keep no shell block that needs files outside the repository. The inventory and oracle files are outside this file's scope, so these edits are listed for the lead.
- **Verification** (e2e): `uv run ruff check . && uv run ruff format --check .` pass with `port/` present. On a clean clone of the branch with the port files committed, `uv run python scripts/check_doc_commands.py` passes (or reports the two blocks as skipped with reasons), and so does `scripts/verify_all.sh`.
- **Status:** resolved: the Phase 7 reviewer ran the Python gate `scripts/verify_all.sh` on its own clean clone of commit 7b7ab2f, with the port committed (T-135): exit 0, "verify_all passed"; ruff, ruff format (248 files) and mypy (101 source files) clean, `pytest -m "" -n auto` 3985 passed, golden, sources and conflicts checks clean, wheel built and checked offline (`docs/port/REVIEW.md` §1, C2). The rest was done before: Ruff, ruff format and mypy were clean with `port/` present before the first port commit (`docs/port/progress.md`, Phase 0). The nine shell-fenced blocks of the Phase 6 records that had no doc-check marker are fixed (`docs/port/eval/weakness-status/GAP-D.md`): the eight fenced ones carry a `<!-- doc-check: skip <reason> -->` marker, and the P6-N inline mention no longer starts with a fence. `port/oracle/export_cli.py`, the one file `ruff format --check .` failed on at 1007ca9, is formatted.
- **Evidence:**
  - `uv run ruff check .` (all checks passed), `uv run ruff format --check .` (233 files formatted), `MYPYPATH=src uv run mypy --strict port/oracle` (33 files) and `uv run mypy` (101 files), before the first port commit.
  - On a clean clone at 54c2bd2, inside the `--full` gate of W-118 (GAP-D): `ruff check`, `ruff format --check` (245 files) and `mypy --strict port/oracle` clean, and the documented commands of the whole repository: 19 blocks run, 11 skipped, 0 failed.
  - `uv run python scripts/check_doc_commands.py --list` (run 2026-11-04): every bash block under `docs/port/` is skipped with its reason, the eight GAP-D markers included.
- **Ablation:** no AI effect (row N).

### W-155 — Change detection ignores edits inside stripped parentheticals, so a reworded semantic parenthetical passes the refresh diff, the golden check and coverage
- **Category:** tooling · **Resolution type:** data tripwire + tooling (Python side, handed to the lead)
- **Sources:** `src/gcg_sim/tools/refresh.py:206-221`, `src/gcg_sim/tools/refresh.py:865-869`, `src/gcg_sim/effects/text.py:17-27`, `src/gcg_sim/effects/text.py:67-95`, `src/gcg_sim/effects/golden.py:40-41`, `.claude/skills/gcg-refresh-data/references/card-checklist.md:122`, `.claude/skills/gcg-refresh-data/SKILL.md:219-223`
- **Quotes:** “A `typography` change needs no work: the golden `text_hash` stays the same.”; “"kind": "typography" if old_norm == new_norm else "wording",”
- **What is weak:** `classify_effect` calls an effect edit `typography` whenever `normalize()` returns the same text, and the refresh skill says such a change needs no work. `normalize` strips every top-level parenthetical that `_is_reminder` accepts: one that ends with "." or has four or more words, and is not a trait. `text_hash` hashes the normalized text. The golden record's `text_hash` and the coverage report's TEXT CHANGED check (`refresh.py:865-869`) use the same hash, and the compiler reads the normalized text, so `golden --check` passes too. But three stripped parentheticals carry meaning that the bindings implement (W-60): GD05-089 "(Don't treat it as a Pilot.)", ST12-013 and ST14-014. Reproduced this session: rewording GD05-089's parenthetical to "(Treat it as a Pilot as well.)" leaves `normalize` and `text_hash` unchanged, and `classify_effect` returns `typography`. An erratum that rewords one of them would be reported as "no behaviour change", and the binding would go stale unnoticed. W-60's planned gate checks which cards have such parentheticals, not what they say.
- **Resolution plan:** a Rust data-gate test extracts every parenthetical that `normalize` strips. A scan this session found 16 distinct texts after normalisation. Thirteen restate a keyword or a card-type rule: the keyword reminders (one is the old <Breach> wording), and the Resource, EX Resource and EX Base rule texts (one, on EXRP-001, is a variant of the EX Resource setup text). The other three are exactly W-60's. The 13 go into a reviewed allowlist in the port's test data, each entry naming its keyword or rule. Every other stripped parenthetical is stored verbatim per card in a snapshot. The gate fails when a card gains a stripped parenthetical that is neither allowlisted nor in its snapshot, or when a snapshot text changes, and it prints the old and new text. On the Python side, a `parenthetical: review` class for such edits in the refresh diff is handed to the lead.
- **Verification** (unit + negative control): the gate passes on the packaged data, with a snapshot of exactly GD05-089, ST12-013 and ST14-014. In a temporary data copy, rewording GD05-089's parenthetical fails the gate, naming GD05-089. A typographic change to a keyword reminder (a curly apostrophe, extra spaces), which `normalize` removes, still passes. A keyword reminder reworded to a text that is not in the allowlist fails, so the new wording is reviewed once.
- **Status:** resolved (Rust side): the parenthetical gate. A Rust port of `normalize`'s stripping extracts every stripped parenthetical. 13 reviewed reminders are allowlisted, and the other three form a per-card snapshot: GD05-089, ST12-013 and ST14-014. Running the gate inside the refresh workflow is a scheduled M17 step (§12.11).
- **Post-acceptance:** the `parenthetical: review` class in `src/gcg_sim/tools/refresh.py` (§12.11 answer (a)).
- **Evidence:**
  - `crates/gcg-data/tests/p6k_data_tripwires.rs::the_text_port_reproduces_every_cards_text_hash`.
  - `w155_stripped_parentheticals_equal_python`: 16 distinct texts.
  - `w155_parenthetical_gate_passes_on_the_packaged_data`.
  - `w155_parenthetical_gate_negative_controls`: rewording GD05-089 fails, a typographic change passes, and a reworded reminder fails.
- **Ablation:** no AI effect (row K).

### W-156 — Replay files and games.ndjson records carry no data, rules or code version
- **Category:** reporting · **Resolution type:** Rust fix (diagnostics, output bytes unchanged)
- **Sources:** `src/gcg_sim/runner/replayfile.py:21`, `src/gcg_sim/runner/replayfile.py:104-148`, `src/gcg_sim/runner/replayfile.py:164-200`, `src/gcg_sim/runner/records.py:79-101`, `src/gcg_sim/reports/writer.py:54-57`, `src/gcg_sim/reports/versions.py:56-68`, `src/gcg_sim/cli.py:92-94`, `docs/REPORTS.md:106-108`
- **Quotes:** “Each replay is self-contained. It holds both decklists, the game seed, the chooser, and the”; “"version": REPLAY_VERSION,”
- **What is weak:** `replay_document` writes the document format and `REPLAY_VERSION = 1` (the version of that format), the seeds, decklists, agents, actions, the expected outcome and the log. It writes no `dataset_version`, rules version, B&R date, `code_digest` or package version. `GameRecord`, which is the line format of `games.ndjson`, has none of these either. Only `results.json` records versions (`versions.py:56-68`). After a data refresh, an override edit or an engine fix, `check_replay` fails with "action N … is not legal in the replayed game" or a `final_state_sha256` mismatch, and nothing says that the versions differ. A user cannot tell a regression from an expected change. W-89 and W-91 cover `results.json` only, and W-148 covers which games are saved.
- **Resolution plan:** replay and `games.ndjson` bytes stay unchanged (equivalence). The Rust `gcg-sim replay` looks for the run's `results.json`: one directory above `replays/`, or a path given with `--results`. It compares that file's `versions` with the running build's and names every difference (field, recorded value, current value). On a failed check, the message says whether the versions differ (an expected change is possible) or match (a regression or nondeterminism). Without a `results.json`, it says that the versions are unknown. When the check passes and the versions match, the output is what Python prints. W-148's `replay --games-ndjson`, which already takes `--results`, uses the same comparison.
- **Verification** (unit + e2e): a replay recorded on a synthetic data copy with a changed `dataset_version`, checked against the packaged data, fails and names the `dataset_version` difference. A replay checked with matching versions passes with Python's output. A replay file copied out of its run directory reports that the versions are unknown. Replay and `games.ndjson` bytes equal Python's.
- **Status:** resolved: `gcg-eval replay-check` runs `gcg-sim replay`'s check and compares the run's `versions` with this build's. On a match it prints exactly `gcg-sim replay`'s output. On a difference it names every differing field; otherwise it says "versions match ... a regression or nondeterminism", or "the versions are unknown". `gcg-sim replay`, replay files and `games.ndjson` are unchanged.
- **Option location (A30):** `gcg-eval replay-check FILE [--results R] [--log]`.
- **Evidence:** `crates/gcg-eval/tests/p6m_cli.rs::w156_replay_check_names_version_differences`.
- **Ablation:** no AI effect (row M).

### W-157 — The card database silently loads with no overrides when overrides.json is missing
- **Category:** data · **Resolution type:** Rust fix (fail fast)
- **Sources:** `src/gcg_sim/cards/db.py:62-68`, `src/gcg_sim/cards/db.py:328-329`, `src/gcg_sim/tools/explain.py:26-30`
- **Quotes:** “return {"schema_version": 1, "policies": {}, "resolutions": {}}”
- **What is weak:** `get_card_db()` builds `CardDB(load_raw_printings(), load_overrides())`, and `load_overrides` returns an empty resolution set when `overrides.json` is not found. If the packaged file were absent (after a packaging error, for example), all 402 resolutions (counted this session) would silently stop applying. They include the ST12-001 errata field override (W-79), the GD02-053, GD03-015 and GD03-035 text fixes (W-80), the null-AP fills (W-82) and the canonical-printing choices (W-83, W-86), so card behaviour would change with no error. `explain._conflicts` falls back the same way. The path is dead today, because the file is packaged and hashed in the lock.
- **Resolution plan:** the Rust loader treats a missing, unreadable or unparsable overrides file as a hard error that names the file. There is no empty default. The data gate's lock check (W-85) also fails on a missing file.
- **Verification** (unit): in a temporary data copy without `overrides.json`, loading fails with an error naming the file. A copy with truncated JSON fails with a parse error naming it. The packaged data loads 402 resolutions.
- **Status:** resolved by construction: the overrides file is embedded with `include_str!`, so a missing file fails the build. `CardDb::from_sources` fails on unparsable or ill-shaped overrides, naming `overrides.json`.
- **Evidence:** `crates/gcg-data/tests/p6k_data_tripwires.rs::w157_packaged_overrides_load_every_card_data_resolution` (402 resolutions; empty and truncated files fail) and `crates/gcg-data/tests/synthetic.rs::overrides_never_fall_back_to_empty`.
- **Ablation:** no AI effect (row K).

### W-158 — A damage-prevention threshold that is not a literal integer is silently read as unlimited prevention
- **Category:** engine · **Resolution type:** Rust fix (fail at registry build) + data tripwire
- **Sources:** `src/gcg_sim/engine/core.py:503-530`, `src/gcg_sim/effects/dsl.py:631-634`
- **Quotes:** “limit = r.rule.amount if isinstance(r.rule.amount, int) else 0”
- **What is weak:** `_prevented` honours a CANT_RECEIVE_DAMAGE threshold ("damage of N or less", GD05-123, Q416) only when `RuleMod.amount` is a literal int. `amount` is typed `Value` (`dsl.py:634`), so it could be a computed value, and any non-int becomes limit 0, which means "can't receive any damage". A scan of every card script in the registry this session found 125 RuleMods, 25 of them CANT_RECEIVE_DAMAGE (on 24 card definitions), and none with a non-integer amount. So the fallback is dead with today's pool, but a future card with a computed threshold would compile to total immunity with no error. W-55 lists other preconditions that hold only for today's pool, but not this one.
- **Resolution plan:** Rust keeps the behaviour for every literal amount. The registry build rejects a CANT_RECEIVE_DAMAGE RuleMod whose amount is not a literal integer, with an error naming the card (W-69 makes every such build error fail fast), so the card fails loudly instead of becoming immune. Evaluating a computed threshold would be a rules change, so it goes to the Python workflow if a card ever needs it. A data tripwire lists every RuleMod whose amount is not a literal, and fails when the list is not empty.
- **Verification** (unit + negative control): the tripwire passes on the packaged pool (0 non-literal amounts). As a negative control, a synthetic card in a temporary data copy whose threshold is Count-based fails the tripwire, and its registry build names the card. GD05-123's literal threshold behaves as in Python (the ported Q416 test).
- **Status:** resolved: the registry build refuses a CANT_RECEIVE_DAMAGE `RuleMod` whose `amount` is a node, with `IrError::Malformed` (`crates/gcg-effects/src/ir.rs`). Literal thresholds behave as before.
- **Evidence:**
  - `crates/gcg-effects/tests/p6j_ir_tripwires.rs::w158_no_prevention_threshold_is_computed`: 0 non-literal amounts, and 25 CANT_RECEIVE_DAMAGE on 24 definitions.
  - `w158_a_counted_threshold_fails_the_tripwire_and_the_build`: the GD05-123 negative control.
  - `w158_the_packaged_registry_builds`.
  - The ported Q416 test in the traces.
- **Ablation:** no AI effect (row J). The packaged registry is unchanged.

### W-159 — Turn-history conditions match their filters against a card's current state, not its state when the event happened
- **Category:** engine · **Resolution type:** port-preserve + data tripwire
- **Sources:** `src/gcg_sim/engine/view.py:914-930`, `src/gcg_sim/engine/state.py:415-426`, `src/gcg_sim/engine/observe.py:86-95`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:212-216`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:646-650`, `src/gcg_sim/effects/bindings/wp_gd03_a.py:555-559`, `src/gcg_sim/effects/bindings/wp_gd03_a.py:695-700`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:610-621`, `docs/port/inventory/06b-engine-game-battle-observe.md:1018-1020`
- **Quotes:** “if c.filters and (h.uid < 0 or not matches(st, dv, ctx, h.uid, c.filters)):”
- **What is weak:** `HappenedThisTurn` counts this turn's history events. It checks `filters` against the event's card (`h.uid`) and `source_filters` against its source, both in the current derived view. `HistoryEvent` stores the card's `def_id` at the time of the event, but no condition reads it. Only `observe._own_history` touches the field, to rewrite it for determinized copies, which is why that rewrite is harmless today. So a card whose characteristics changed after the event stops matching the filter, or starts to. Five bindings filter on characteristics: GD05-129 (a destroyed UNIT, by a Neo Zeon source), GD03-081 (a deployed Unit with a trait), GD04-069 (a cost paid for a Militia or Dianna Counter Unit), GD03-069 (paired to this card) and GD05-089 (a Special Move Command activated). Example: GD05-089 deployed "as an (AP3･HP3) Unit" and then destroyed is a Pilot card again in the trash, so GD05-129's UNIT filter would not count it. The same applies to a trait or keyword granted by an effect that has since ended. This is inferred from the code. It has not been reproduced or checked against an official ruling. W-57's "turn-history gaps" were missing events, which is a different issue.
- **Resolution plan:** Rust keeps the behaviour exactly. Changing it is a rules interpretation, so it goes to the Python conflict workflow as a curated conflict. A pinned testkit case records Python's result for GD05-089 deployed as a Unit, then destroyed by a Neo Zeon card's effect, with GD05-129 in play. If no current Neo Zeon effect can destroy a friendly Unit, the case uses a test-only binding, and the entry records that the pool cannot reach it. A data tripwire lists every `HappenedThisTurn` filter that reads a characteristic that can change after the event (card kind, traits, keywords, pairing, stats), and fails when the list changes, so each new use is reviewed.
- **Verification** (unit + negative control): the pinned case gives Python's result in Rust. The tripwire lists exactly the five bindings above on the packaged pool. As a negative control, a synthetic binding in a temporary copy with a trait filter on a "discard" history condition fails it.
- **Status:** resolved (port-preserve and tripwire): the behaviour is kept exactly. It is a rules interpretation, for the Python conflict workflow.
- **Evidence:**
  - `crates/gcg-engine/tests/p6j_engine.rs::w159_a_unit_that_is_a_pilot_again_does_not_count`: the pool case GD05-089, GD05-057 and GD05-129, equal to Python.
  - The tripwire `crates/gcg-effects/tests/p6j_ir_tripwires.rs::w159_history_filters_on_characteristics_are_reviewed` lists GD03-069, GD03-081, GD04-069, GD05-089 and GD05-129.
  - The negative control `w159_a_trait_filter_on_a_discard_history_fails_the_tripwire`.
- **Ablation:** no AI effect (row J).

### W-160 — The Python-vs-Rust equivalence harness compares results.json only up to JSON formatting, skips decision logs and the fast preset, and can reuse a stale Python run
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `scripts/normalize_outputs.py:2-6`, `scripts/normalize_outputs.py:22-27`, `scripts/compare_python_vs_rust.sh:2`, `scripts/compare_python_vs_rust.sh:75`, `scripts/compare_python_vs_rust.sh:130-182`, `src/gcg_sim/reports/writer.py:33-34`, `docs/port/progress.md:5`
- **Quotes:** “return json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + "\n"”; “local matches=10 seeds="1 2 3" formats="bo1 bo3" presets="standard strong" workers="1 4 10"”
- **What is weak:** the ground rules and W-98 rely on byte-identical outputs "as compared through `scripts/normalize_outputs.py`", and the harness has these gaps:
  - `normalize_results` parses `results.json` and writes it again with Python's serialiser (the same call as `writer.to_json_text`). That erases every difference in Rust's own serialisation: float digits or exponent form, key order, escaping, indentation, the trailing newline. So `results.json` is compared only up to JSON formatting.
  - `cmd_equivalence` defaults to the presets `standard strong` (no `fast`) and never passes `--decision-log`, so decision-log bytes in `games.ndjson` and replays are never compared.
  - Python always runs with `--workers 10` and the inherited PYTHONHASHSEED. The two-hash-seed generation that W-150's plan requires is not in the harness.
  - A cached Python run is reused whenever its `results.json` exists, with no check of the Python commit or the data lock, so a stale baseline can be compared.
  - The scripts cite `docs/port/BENCHMARKS.md` and `docs/port/EQUIVALENCE.md`, and `docs/port/progress.md` cites `docs/port/tests.json`. None of these files exists (checked this session).

  W-154 covers only the Python lint and doc gates.
- **Resolution plan:**
  1. Compare raw bytes. The normaliser masks only the three implementation fields (`versions.package`, `versions.code_digest`, `ai.factory`) by exact text substitution of their values in `results.json` and `summary.md`, and copies every other byte.
  2. Add `fast` to the default presets, and run every configuration both with and without `--decision-log`.
  3. Generate each Python baseline twice, under PYTHONHASHSEED=1 and PYTHONHASHSEED=2, and require the two to be byte-identical before Rust is compared against them (W-150).
  4. Record the Python commit and the data-lock hash next to each cached Python run, and regenerate the run when either differs.
  5. Write the cited docs, or remove the references.
- **Verification** (unit + negative control): as negative controls, a Rust `results.json` with one float written as `1e-5` instead of Python's `1e-05` fails the comparison, and so does one without the trailing newline. A cached Python run whose recorded commit differs from HEAD is regenerated. The harness's summary counts configurations per preset and decision-log setting, and the default matrix includes `fast` and decision-log runs. Every doc path the scripts cite exists.
- **Status:** resolved: the Phase 3 harness of the comparison script compares raw bytes, masking only the three implementation fields. It detects stale baselines, and its matrix includes `fast` and decision-log runs.
- **Evidence:** `scripts/compare_python_vs_rust.sh equivalence --selftest`: 15/15 (T-032), including `1e-5` against `1e-05` and a missing newline. `report --selftest`: 14/14 (T-158). The M9 matrix: 324/324 byte-identical (T-002).
- **Ablation:** no AI effect (row N).

### W-161 — ToDeck can offer duplicate ARRANGE options when a targeted Unit's Pilot is also a target
- **Category:** engine · **Resolution type:** port-preserve + data tripwire
- **Sources:** `src/gcg_sim/engine/interp.py:781-810`, `src/gcg_sim/engine/interp.py:813-830`, `docs/port/inventory/06a-engine-core-interp.md:1186-1188`
- **Quotes:** “moved = [u for u in (*targets, *pilots) if st.cards[u].zone is Zone.DECK]”
- **What is weak:** `_h_to_deck` adds the Pilots of targeted battle-area Units to `moved` without removing duplicates. When a Pilot is also a target itself, `moved` and `together` hold it twice. `_todeck_next` then offers duplicate SELECT options in the owner's ARRANGE decision. Once the other cards are placed, it can ask one more ARRANGE decision whose two options are the same card. The cards end up in the same places, but the decision sequence changes: replay action lists, decision counts and AI branching. Whether the current pool can reach this is not established (inventory 06a, open question 1), and no other entry covers it.
- **Resolution plan:** Rust keeps the behaviour exactly, duplicates included, so replays and digests stay equal. A data tripwire scans the IR for ToDeck steps whose selector can choose a paired Pilot together with its Unit, and lists them. If the list is not empty, a testkit regression pins the duplicate options and the extra decision, and the case goes to the lead as a possible Python bug, to be fixed in both implementations together.
- **Verification** (unit): the tripwire runs on the packaged pool and its result is recorded. A testkit position with a test-only ToDeck that targets a Unit and its Pilot gives the same ARRANGE options (duplicates included) and the same number of decisions in Rust as in Python.
- **Status:** resolved (port-preserve and tripwire): the behaviour is kept exactly, duplicates included.
- **Evidence:**
  - The tripwire `crates/gcg-effects/tests/p6j_ir_tripwires.rs::w161_no_todeck_can_return_a_unit_with_its_pilot`: none of the 7 ToDeck steps can return a paired Pilot.
  - The negative control `w161_a_unit_and_its_pilot_fail_the_tripwire`.
  - `crates/gcg-engine/tests/p6j_engine.rs::w161_duplicate_arrange_options_are_pythons`: Python's ARRANGE options and decision counts.
- **Ablation:** no AI effect (row J).

### W-162 — Trigger and frame fields that are serialised but never read (host_seq, optional, answer, Battle.blocked) may hide last-known-information bugs
- **Category:** engine · **Resolution type:** port-preserve + fuzz diagnostic
- **Sources:** `src/gcg_sim/engine/interp.py:61-77`, `src/gcg_sim/engine/core.py:248-283`, `src/gcg_sim/engine/core.py:354`, `src/gcg_sim/engine/game.py:712-727`, `src/gcg_sim/engine/state.py:145-156`, `src/gcg_sim/engine/state.py:266-278`, `src/gcg_sim/engine/state.py:326-339`, `src/gcg_sim/runner/digest.py:36-46`, `src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md:313`, `docs/port/inventory/06b-engine-game-battle-observe.md:1014-1017`, `docs/port/inventory/06a-engine-core-interp.md:1200-1201`
- **Quotes:** “host_seq = st.cards[host].zone_seq if host >= 0 else 0”
- **What is weak:** for a trigger of a card that is leaving play, `queue_trigger` receives the host's last-known `zone_seq` (`core.py:354`) and stores it in `TriggerInst.host_seq`. It uses the value for the 【Once per Turn】 key (`core.py:270`), but nothing else reads the field. `_resolve_trigger` does not pass it on, and `push_frame` recomputes `Frame.host_seq` from the host's current `zone_seq` (`interp.py:72`). A grep this session found no reader of `TriggerInst.host_seq` or `Frame.host_seq` outside serialisation. So a trigger whose host moved again between queueing and resolution resolves against the card in its new location, although rule 4-1-5 treats it as a new card there. Whether any current card reaches this is not established; it is plausible from the code. `Battle.blocked`, `damage_only` and `attack_triggers`, `TriggerInst.optional`, `Frame.answer` and `vars["__play"]` are also written, and hashed into the final digest through `GameState.to_json` (`digest.py:36-46`), but never read. W-54 covers only the PAYMENT decision kind and the ATTACK_TRIGGERS step.
- **Resolution plan:** Rust keeps every field, and its serialised position, for digest identity, and documents the unread ones as "serialised for digest identity, not read". In paranoid mode (W-49), the Rust engine records each host's `zone_seq` at the moment its trigger is queued, in a side table outside the game state, so digests are unchanged. It compares that value with the host's `zone_seq` when the trigger resolves, and counts and lists the cards where they differ. (Comparing with the queued `TriggerInst.host_seq`, as first proposed, would flag every 【Destroyed】 trigger, because that field holds the pre-move `zone_seq`.) Any card the fuzz finds goes to the Python conflict workflow as a rules question: should the effect use the card in its new location, last-known information, or do nothing? Only its resolution may change the behaviour, in both implementations together.
- **Verification** (unit + fuzz): unit: a testkit position where a host moves again before its trigger resolves increments the counter. One example is a 【Destroyed】 trigger whose card an earlier trigger of the same batch returns from the trash to the hand; a test-only binding may build it if no card reaches it. An ordinary 【Destroyed】 trigger does not increment the counter. Fuzz: 10,000 games in paranoid mode report the counter and list every card that triggered it, and final digests equal Python's.
- **Status:** resolved (port-preserve and fuzz diagnostic): every field stays. In `paranoid` builds, a trigger whose host moved again before it resolved is recorded (`gcg_engine::paranoid::moved_hosts()`). The 10,000-game paranoid fuzz recorded 395 such triggers, in 350 games, on 29 cards. They read as battle-damage triggers whose host the same damage step destroyed. Per the plan they go to the Python conflict workflow as a rules question, listed for the lead. The behaviour is unchanged.
- **Evidence:** `crates/gcg-engine/tests/p6j_engine.rs::w162_a_host_that_moves_before_its_trigger_resolves` and `crates/gcg-diff/tests/p6j_runs.rs::p6j_fuzz_diagnostics_on_all_10000_python_fuzz_games`.
- **Ablation:** no AI effect (row J). The side table exists only in `paranoid` builds.

### W-163 — The Burst acceptance score assumes that no Burst should be declined, which was checked only on the example and test decks
- **Category:** ai-policy · **Resolution type:** AI (strengthened search) + data tripwire
- **Sources:** `docs/python/AI.md:161`, `src/gcg_sim/ai/config.py:67`, `src/gcg_sim/ai/policy.py:309-310`
- **Quotes:** “no Burst in the example or test decks should be declined”
- **What is weak:** with `policy_burst_yes_score = 4.0` in the presets, a rollout at temperature 0.5 accepts a Burst 99.97% of the time (it declines 0.03%). The stated justification covers only the example and test decks. A scan this session of the 252 【Burst】 lines in the packaged pool (one per card) found none with an obvious drawback: the lines that mention the trash or "instead" are benefits, such as returning a card from the trash to the hand. So the assumption holds today. Nothing checks it for future cards, though: a Burst with a cost, a step that harms its controller's own cards, or an "instead" choice would be accepted in almost every rollout. The root decision is searched, but rollouts and the modelled opponent use the policy. W-27 treats Bursts as fixed and plans only for generic YES_NO decisions.
- **Resolution plan:** the reference keeps the score. A data tripwire lists every Burst whose IR contains a cost, a step whose chosen or affected cards include its controller's own (other than adding cards to the hand or deploying this card), or an optional replacement. It fails when the list changes, so each new one is reviewed. The strengthened policy scores the Bursts on that list with W-24's one-ply look-ahead, as W-27 does for YES_NO, and keeps the high score for the others.
- **Verification** (unit + negative control): the tripwire's list on the packaged pool is recorded and reviewed. It is expected to hold GD05-089, whose "you may deploy it … instead" is an optional replacement, and no harmful Burst. As a negative control, a synthetic Burst "Discard 2 cards. Draw 1." in a temporary data copy fails the tripwire. Unit tests on rollout distributions: the strengthened policy declines that synthetic Burst at least 90% of the time in a position where the look-ahead shows a loss, and accepts the packaged Bursts at least 99.9% of the time.
- **Status:** resolved: the data tripwire is in force. `burst_decline_list` answers a listed Burst by look-ahead. It measured null and was in `B_mid`, so it is off in both presets; it changes nothing on the packaged decks and stays available for future cards.
- **Evidence:**
  - `burst_tripwire_holds_on_the_packaged_pool`: the reviewed list is exactly `GD05-089: optional`.
  - The negative control `burst_tripwire_fails_on_a_synthetic_harmful_burst`.
  - `burst_decline_list_declines_a_harmful_burst_and_accepts_packaged_ones`: on the synthetic Burst the reference says YES 19,994/20,000 and `burst_decline_list` 42/20,000; on the packaged ST02-014 and GD05-089 it says YES 19,994/20,000 each.
- **Ablation:**
  - Individual (dev 20600100): 199/400 (0.497, [0.449, 0.546]), -0.005 [-0.076, +0.066], discordant 26/27, 9772 vs 9744 / 117734 vs 117811, identical to the placebo in all 400 games (`docs/port/eval/P6-F/rows/dev-burst_decline_list-400.json`).
  - Bundle: in `B_mid`.

### W-164 — Token-definition fallbacks and two override-loader forms are never exercised by current data
- **Category:** data · **Resolution type:** Rust fix (implemented with synthetic-data tests) + data tripwire
- **Sources:** `src/gcg_sim/cards/db.py:80-97`, `src/gcg_sim/cards/db.py:131-138`, `src/gcg_sim/cards/db.py:199-241`, `docs/port/inventory/03-data-deck.md:935-937`
- **Quotes:** “card_number=f"TOKEN:{s.name}:{s.ap}/{s.hp}",”; “Synthetic `TOKEN:` defs never occur with the current data, and the `field_overrides`”
- **What is weak:** an inline token definition resolves to the lowest-numbered token card with the same name, AP, HP and traits. If none matches, a synthetic `TOKEN:name:ap/hp` card is created. A check this session found that no two of the 29 token cards share name, stats and traits, and that no definition needs a synthetic card, so both the tie-break and the synthetic path are dead today. Two override forms are unused too (inventory 03, open question 4): the nested per-number `field_overrides` form (`db.py:136-137`) and the singular `card_number` resolution key (`db.py:90-92`). A refresh that triggers one of these paths would take an untested route. The third fallback, which infers the card number from the conflict id (`db.py:93-94`), is live, contrary to the review that proposed this entry. Fifty resolutions that change card data, such as the canonical-printing choice `divergent:EXB-001`, have no card-number field and rely on it (counted this session). W-54 covers dead enum values only.
- **Resolution plan:** Rust implements all of these paths exactly. Each dead path gets a synthetic-data unit test, with expected values exported from Python: a token definition with no matching card, two matching token cards, a nested per-number override, and a singular `card_number` key. The live id inference is covered by the packaged data. A data tripwire fails when the packaged data first triggers a dead path, so the change is reviewed.
- **Verification** (unit + negative control): the synthetic tests give Python's card definitions (number, name, stats and `def_id` order). The tripwire passes on the packaged data, and each synthetic copy above makes it fail, naming the path. For all 50 id-inferred resolutions, the packaged data resolves the same card data as Python.
- **Status:** resolved: the dead loader paths are implemented and tested on synthetic data with Python's expected values. The tripwire fails when the packaged data first takes one.
- **Evidence:**
  - `crates/gcg-data/tests/p6k_data_tripwires.rs::w164_dead_loader_paths_stay_dead_with_negative_controls`.
  - `crates/gcg-data/tests/synthetic.rs::synthetic_sources_load_as_in_python`.
  - `carddefs.rs::every_card_def_equals_python_field_by_field`.
- **Ablation:** no AI effect (row K).
