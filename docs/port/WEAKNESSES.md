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
  sideboarding and clock (W-131 to W-133). Every other entry has a plan and the status `open`.
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

## Summary

| ID | Title | Category | Resolution type | Verification |
| --- | --- | --- | --- | --- |
| W-01 | Two-turn rollout horizon: plans that pay off later rest on the evaluation | ai-search | AI (strengthened search) | scenario + ablation |
| W-02 | Leaf-value constants cap the horizon at four turns | ai-search | AI (strengthened search) | unit |
| W-03 | Lethal-threat defence: the audited "keep the last Blocker" position is unsolved; threat-aware rollouts were measured null and removed | ai-search | AI (strengthened search) | scenario + ablation |
| W-04 | Budget plateau: strength is model-limited past about 2x budget; `strong` is barely stronger than `standard` | ai-search | AI (strengthened search) + eval tooling | ablation + unit |
| W-05 | Strategy fusion: opponent nodes pool statistics across hidden worlds (single-observer ISMCTS) | ai-search | AI (strengthened search) | scenario + ablation |
| W-06 | Observation keys count one child twice when the opponent passes twice in a row | ai-search | AI (strengthened search) | unit + ablation |
| W-07 | Determinization samples hidden cards uniformly: no inference from the opponent's actions | ai-search | AI (strengthened search) | scenario + ablation + unit |
| W-08 | Determinization pads or trims pools when effects move cards between players' decks | ai-search | AI (strengthened search) + Rust fix (diagnostic) | unit + fuzz |
| W-09 | Move canonicalization never pays with an EX Resource on purpose | ai-search | AI (strengthened search) | scenario + unit + ablation |
| W-10 | Equivalent-move merge fingerprint is conservative and depends on instance numbers | ai-search | AI (strengthened search) | unit + property |
| W-11 | Pruning and merging run only at the root; no transposition or tree reuse | ai-search | AI (strengthened search) | ablation + unit |
| W-12 | Root pruning is judged in one fixed determinization by the deterministic policy, with silent caps | ai-search | AI (strengthened search) | scenario + unit + ablation |
| W-13 | The "while rested" Rest-target exception was added after the measurements and never fired at game level | ai-search | AI (strengthened search) + eval tooling | scenario + e2e |
| W-14 | Close target choices stay noisy: the <Repair> ping puzzle is solved about a third of the time | ai-search | AI (strengthened search) | scenario + ablation |
| W-15 | Undocumented rollout cap of 250 decisions: capped rollouts are evaluated mid-turn | ai-search | AI (strengthened search) + eval tooling | unit + e2e |
| W-16 | Budget is fixed by option count; only effect-target decisions may extend | ai-search | AI (strengthened search) | scenario + ablation |
| W-17 | Remaining implausible plays: passivity, questionable attacks, missed lethals; the audit sample is small | ai-search | AI (strengthened search) + eval tooling | scenario + ablation |
| W-18 | `fast` rates are not `standard` rates; fidelity rests on one weakened deck | ai-search | AI (strengthened search) + eval tooling | e2e |
| W-19 | Strength acceptance bars are thin floors | ai-search | eval tooling (acceptance gates) | e2e |
| W-20 | Strength evidence rests on few decks, and the tuning decks overlap the example decks | ai-search | eval tooling (deck panel) | unit + e2e |
| W-21 | The decision log keeps the top 5 alternatives and does not record search effort | ai-search | eval tooling (instrumentation) | unit + e2e |
| W-22 | Greedy baseline: one determinization, baseline knobs, fixed choices, silent cap | ai-policy | port-preserve + eval tooling | e2e + unit |
| W-23 | Hand-written playout policy: Commands under-cast, abilities over-activated, unfitted constants | ai-policy | AI (strengthened search) | ablation + e2e |
| W-24 | Modal choices are scored flat | ai-policy | AI (strengthened search) | scenario + unit + ablation |
| W-25 | The opponent is modelled with the searcher's own policy | ai-policy | AI (strengthened search) | scenario + ablation |
| W-26 | Kill-aware and damage-payoff targeting were measured and removed | ai-policy | AI (strengthened search, re-measure) | ablation |
| W-27 | YES_NO answers say no about 12% of the time; the policy always goes first and keeps | ai-policy | AI (strengthened search) | unit + ablation |
| W-28 | End-phase action-step plays are under-explored by rollouts | ai-policy | AI (strengthened search) | scenario + ablation |
| W-29 | No policy scorer for ORDER_TRIGGER decisions | ai-policy | AI (strengthened search) | scenario + unit |
| W-30 | Target polarity classification is coarse | ai-policy | AI (strengthened search) | unit + ablation |
| W-31 | Heuristic card and fight values are rough | ai-policy | AI (strengthened search) | unit + ablation |
| W-32 | AI Blocker counts ignore CANT_BLOCK and NO_BLOCKER_VS | ai-policy | AI (strengthened search) | unit + scenario |
| W-33 | AI module-level caches are unbounded and not invalidated on script override | ai-policy | Rust fix (no behaviour change) | unit |
| W-34 | Evaluation fitted on pre-change search games; refit and per-preset fits deferred | ai-evaluation | AI (strengthened search) | ablation + e2e |
| W-35 | The evaluation is linear; interaction terms and other candidate features gained nothing | ai-evaluation | AI (strengthened search) | ablation + unit |
| W-36 | The evaluation was trained on three tuning decks and transfers weakly to other decks | ai-evaluation | AI (strengthened search) + eval tooling | e2e |
| W-37 | The evaluation ignores most keywords, card text and effects | ai-evaluation | AI (strengthened search) | unit + ablation |
| W-38 | can_kill and race features are crude board heuristics | ai-evaluation | AI (strengthened search) | unit + ablation |
| W-39 | The negative tempo weight is an artifact of when positions are recorded | ai-evaluation | port-preserve + AI (strengthened search) | unit |
| W-40 | Weight-tuning methodology limits | ai-evaluation | eval tooling | unit |
| W-41 | Matchup estimates are not calibrated against real tournament results | ai-evaluation | eval tooling (data from the user) | e2e |
| W-42 | No paired analysis of same-seed runs; compare reports a pooled Wilson interval | reporting | eval tooling | unit + e2e |
| W-43 | Decision-level regret (oracle replay) tooling was never committed | tooling | eval tooling | unit + e2e |
| W-44 | Play/draw choice: policy-vs-policy simulations, biased to going first, capped simulations scored 0.5 | ai-setup | AI (strengthened search) | unit + e2e |
| W-45 | Mulligan heuristic: greedy four-turn opening score with a fixed 90% threshold | ai-setup | AI (strengthened search) | scenario + ablation |
| W-46 | Safety caps: 200 turns and 20,000 decisions share one end reason; the decision cap is untested; self-play stops earlier with no end reason | engine | Rust fix (additive diagnostics) | unit + e2e |
| W-47 | No guard against decision-free loops inside advance() | engine | Rust fix (error path only) | unit + fuzz |
| W-48 | The derived-view fixpoint is silently capped at six passes | engine | Rust fix (diagnostic, identical output) | unit + fuzz |
| W-49 | The derived-view cache relies on manual touch() calls and a heuristic key; only the paranoid guard checks it | engine | Rust fix (by construction) | unit + fuzz |
| W-50 | The listened-events fast path depends on the decklists; the listen cache is process-global | engine | Rust fix (by construction) | unit + fuzz |
| W-51 | The rules-management HP shortcut depends on a registry-global "no HP-lowering effect" flag | engine | Rust fix (identical output) | unit + fuzz |
| W-52 | GameState.clone shares pending, the derived view and history entries (CB-1) | engine | Rust fix (by construction) | unit + e2e |
| W-53 | Registry indices are process-local, so saved states do not reload faithfully in another process | engine | Rust fix (content-addressed ids) | unit + e2e |
| W-54 | Dead enum values: the PAYMENT decision kind and Step.ATTACK_TRIGGERS | engine | Rust fix (no behaviour change) | fuzz |
| W-55 | Interpretations and N/A rules that hold only for the current card pool | engine | port-preserve + data tripwire | unit + negative control |
| W-56 | Rules-interpretation decisions the port must keep, pinned by rulings and curated conflicts | engine | port-preserve + conformance tests | unit + differential |
| W-57 | Historical engine, validator, runner and tooling bugs fixed in Python: regression risk for the port | engine | port-preserve + regression tests | unit + meta-test |
| W-58 | Most-defensible readings of ambiguous card text and major ruling overrides | effects | port-preserve + reporting | unit + e2e |
| W-59 | Pilot text routing is inferred, not encoded in the data | effects | port-preserve + audit table | unit |
| W-60 | Explanatory parentheticals are stripped but carry semantics the bindings must honour | effects | port-preserve + tests | unit |
| W-61 | Grammar slips and duplicated token clauses in card text break templates | effects | port-preserve + tests | unit |
| W-62 | 'instead' wording: conditional overrides vs true substitutions | effects | port-preserve + tests | unit |
| W-63 | The regex compiler covers about 64% of non-vanilla cards; 302 per-card bindings; new sets need manual multi-agent work | effects | general templates, golden-identical (Rust or Python side, after the IR decision) | unit + differential |
| W-64 | Card-specific custom hooks, several duplicated; custom steps cannot request choices | effects | Rust fix (consolidated hooks, behaviour-identical; form depends on the IR decision) | unit + differential |
| W-65 | The keyword and timing vocabulary is duplicated across many files | effects | Rust fix (single source) | unit |
| W-66 | The compiler miscompiled some cards silently; bindings fixed the cases found | effects | Rust fix (differential audit) | unit |
| W-67 | No templates for "all players each …" and related multi-player-step wording | effects | general templates, golden-identical (Rust or Python side, after the IR decision) | unit + differential |
| W-68 | Stale "compiled to printed HP" binding notes: Stat.HP already means current HP | effects | Rust fix (dedupe, behaviour-identical) | unit |
| W-69 | The registry build swallows binding errors; the card silently becomes unimplemented | effects | Rust fix (fail fast) | unit |
| W-70 | Historical compiler and DSL bugs fixed in Python: regression risk for the port | effects | port-preserve + regression tests | unit + meta-test |
| W-71 | The ST02/ST05 unmodified starter-deck exceptions are not applied | validator | Rust fix (deferred until data exists) + reporting | unit |
| W-72 | One current B&R list: no history and no region | validator | Rust fix (schema-ready, data unchanged) | unit |
| W-73 | Vanilla banned pair: the predicate and the enumerated list can diverge, and divergence only adds a note | validator | data tripwire | unit + negative control |
| W-74 | Validator predicates that hold only for today's pool: "without effects", primary-name matching, one colour per card | validator | data tripwire | unit + negative control |
| W-75 | The B&R list is enforced by default although the Comprehensive Rules never mention it | validator | port-preserve + reporting | unit + e2e |
| W-76 | Tournament-only deck rules, limited formats, tournament procedures and Edition Beta legality are not modelled | validator | reporting (explicit scope, additive outputs) | unit + e2e |
| W-77 | Deck file format limits | validator | Rust fix (diagnostics) | unit |
| W-78 | Legality depends on simulator support (UNIMPLEMENTED) | validator | port-preserve + reporting | unit |
| W-79 | Official errata missing from gcg-api are applied by hand (ST12-001) | data | data tripwire (data unchanged) | unit + negative control |
| W-80 | Card text format defects corrected by overrides (GD02-053, GD03-015, GD03-035) | data | data tripwire | unit |
| W-81 | gcg-api structure fields (keyword_effects, timing_markers, link_refs) are lossy | data | port-preserve + test | unit |
| W-82 | A null printed AP is read as 0 (inference); EX Base AP is null in the source | data | data tripwire | unit + negative control |
| W-83 | Edition Beta stat records are defective (one-row shift in the source listing) | data | data tripwire | unit |
| W-84 | Link, name and trait anomalies in the card data | data | port-preserve + test | unit |
| W-85 | Derived data files are curated by hand; curated conflict texts drifted from their resolutions | data | data tripwire | unit + negative control |
| W-86 | Printing text divergences resolved to the base printing; a wrong rule citation was fixed in review | data | data tripwire | unit |
| W-87 | The Comprehensive Rules file has wrong cross-references and typos (the source is never edited) | data | port-preserve + test | unit |
| W-88 | Provenance gaps: some official sources were read but not stored; cards.json is omitted from the package | data | reporting (provenance listing) | e2e |
| W-89 | dataset_version's sha is the code commit that produced the data, not the data commit | data | port-preserve + docs | unit |
| W-90 | Per-card and benchmark-card statistics are correlations inside AI-vs-AI games | reporting | Rust fix (additive analysis) | unit + e2e |
| W-91 | Results are specific to the AI and preset, and not comparable across AI versions | reporting | Rust fix (guard in comparison tools) | unit |
| W-92 | Game-level intervals and tests treat games within a BO3 match as independent | reporting | Rust fix (additive analysis) | unit |
| W-93 | Draws count as non-wins | reporting | port-preserve + reporting | unit |
| W-94 | Hypothesis error control is per kind of comparison; thresholds are hand-set | reporting | Rust fix (additive analysis) | unit |
| W-95 | "Never played" signals have no significance test or cap, and cost discards count as never played | reporting | Rust fix (additive analysis) | unit |
| W-96 | Benchmark-card "seen" covers only played, Burst-revealed or public cards; lift is one-sided | reporting | Rust fix (additive analysis) | unit |
| W-97 | The results schema file name stays v1 at version 1.1.0 | reporting | port-preserve + docs | unit |
| W-98 | Distribution statistics are integer nearest-rank values and rounded rates | reporting | port-preserve + Rust fix (additive analysis) | unit |
| W-99 | Decision logs make outputs large; reports and logs truncate | reporting | Rust fix (opt-in compression) | unit + e2e |
| W-100 | The summary's reproduce command omits the agent factory and can name a deck that is not a path | reporting | Rust fix | e2e |
| W-101 | conflict_resolutions lists only resolutions whose id names a deck card | reporting | Rust fix (additive field) | e2e |
| W-102 | The report directory is not updated atomically as a set | reporting | Rust fix | unit |
| W-103 | code_digest hashes every file of the installed package; the version can be "unknown" | reporting | Rust fix | unit |
| W-104 | timing.json is wall-clock based and the only non-reproducible report | reporting | Rust fix (additive fields) | unit |
| W-105 | Interrupted or failed runs discard all work: no checkpoint or resume | runner | Rust fix | e2e |
| W-106 | Worker termination: waitpid race, a private executor attribute, AI tools without interrupt handling | runner | Rust fix (by construction) | unit + e2e |
| W-107 | Spawn start-method constraints on the Python API | runner | Rust fix (not applicable by design) | unit |
| W-108 | The GCG_SIM_AGENT_FACTORY test hook can replace the AI in a benchmark | runner | Rust fix | unit + e2e |
| W-109 | Simultaneous defeat and the chooser after a drawn game rest on unconfirmed suggestions | runner | port-preserve + reporting | unit + e2e |
| W-110 | Robustness fuzzing uses only random agents | tooling | tooling | e2e |
| W-111 | Coverage gates count tags, not behaviour | tooling | tooling | unit |
| W-112 | The long slow suite is not deselected by default | tooling | tooling | e2e |
| W-113 | Test helpers stop silently at step caps | tooling | tooling | unit |
| W-114 | The documented-command checker: shell blocks only, a network-dependent skip, timeouts that leave children running | tooling | tooling | unit |
| W-115 | Research artifacts are pinned to an old snapshot and outside the gate; regeneration is manual | tooling | tooling | unit |
| W-116 | Research scripts disagree with the engine on canonical printing and reminder-text stripping | tooling | port-preserve + test | unit |
| W-117 | The adversarial review was a sample | tooling | tooling | e2e |
| W-118 | verify_all.sh gate limits | tooling | tooling | e2e |
| W-119 | blind_transcripts.py uses a private helper and path-dependent ids, untested | tooling | tooling | unit |
| W-120 | Project Claude permissions are broader than the "only the refresh skill uses the network" rule | tooling | configuration change (needs the user's approval) | unit |
| W-121 | Refresh tooling: only the offline core is tested; official fetches need a TLS workaround | tooling | tooling | unit |
| W-122 | Untracked bytecode of an unmerged later branch (bench/, tests/bench, tests/perf) | tooling | tooling (inventory) | unit |
| W-123 | compare.py usability limits | tooling | eval tooling | unit + e2e |
| W-124 | Search is engine-bound; invalidating the derived view by mutation kind is deferred | performance | Rust fix (output-identical) | e2e + unit |
| W-125 | Action and Decision are frozen dataclasses (slow construction on the hot path) | performance | Rust fix (by construction) | unit |
| W-126 | Throughput measurement counts wall time x workers on one pairing; machine state is not recorded | performance | tooling | e2e |
| W-127 | Investigation evidence is not reproducible: raw outputs were gitignored and cited commits were squashed away | docs | docs + eval tooling | unit |
| W-128 | Stale counts across status.json and the research docs | docs | docs (generated counts) | unit |
| W-129 | Hand-maintained docs drift from the code | docs | docs (checked contracts) | unit |
| W-130 | The shuffle-cut and chooser-procedure wording disagree with the engine and REPORTS | docs | port-preserve + docs | unit |
| W-131 | Multiplayer (rules section 12) is out of scope | design-decision | admissible exception | n/a |
| W-132 | No sideboarding in BO3 | design-decision | admissible exception | n/a |
| W-133 | No clock: time limits, extra turns and tiebreaks are not modelled | design-decision | admissible exception | n/a |
| W-134 | Both players know both exact decklists | design-decision | AI (strengthened search, opt-in information mode) | unit + e2e |
| W-135 | Information-set honesty is a convention, not enforced by the API | design-decision | Rust fix (by construction) | unit + compile-fail |
| W-136 | SearchConfig defaults reproduce the pre-change AI; baseline presets live in code | design-decision | Rust fix (separate types) | unit |
| W-137 | The determinism contract; cross-platform identity depends on the C math library | design-decision | port-preserve + tests + portable math (strengthened search); the `erfc` FFI needs the lead's decision | unit + e2e |
| W-138 | Multi-card choices are sequential single picks | design-decision | port-preserve + AI (strengthened search) | unit + ablation |
| W-139 | Opponent nodes are tuned against the searcher's true private cards | ai-search | AI (strengthened search) | scenario + ablation |
| W-140 | In-tree priors come from the first world that reaches each node | ai-search | AI (strengthened search) | unit + ablation |
| W-141 | The no-effect branch check compares a branch with a policy line that may replay the move | ai-search | AI (strengthened search) | unit + ablation |
| W-142 | The evaluation is fitted on true-state features but scores determinized states; Burst density is computed once per search | ai-evaluation | AI (strengthened search) + eval tooling | unit + ablation |
| W-143 | Battle-damage redirect cycles recurse without bound and crash the game (two GD04-095) | engine | Rust fix (error path only) + data tripwire | unit + e2e + negative control |
| W-144 | The deck parser accepts unbounded counts: a huge count crashes with a traceback or allocates without limit | validator | Rust fix (diagnostics) | unit |
| W-145 | The packaged card data predates GD06 (released 2026-10-30), so its cards are unknown | data | Rust fix (data-driven loader) + reporting | unit + e2e |
| W-146 | The rules markdown is checked against the official PDF only by its set of rule ids | data | data tripwire | unit + negative control |
| W-147 | The mulligan-outcome hypothesis is confounded by the AI's own redraw rule | reporting | Rust fix (additive analysis) | unit |
| W-148 | Saved replays are median-length games, and the CLI cannot replay other games.ndjson records | reporting | Rust fix (opt-in) | e2e |
| W-149 | Which failure a parallel run reports depends on completion timing | runner | Rust fix (error path only) | unit |
| W-150 | Determinism acceptance tests run only with the random test agent | tooling | tooling | e2e + negative control |
| W-151 | Fuzz coverage counts cards in decks, not cards executed; tokens and EX cards are never in decks; the seed is fixed | tooling | tooling | unit + e2e |
| W-152 | Seat and first-player symmetry was checked once and has no regression test | tooling | tooling | e2e + negative control |
| W-153 | News and errata discovery in the refresh skill reads only a few listing pages and flags entries by title | tooling | tooling (Python side, handed to the lead) | unit |
| W-154 | Untracked port artifacts would break the project's Python gates once committed | tooling | tooling | e2e |
| W-155 | Change detection ignores edits inside stripped parentheticals, so a reworded semantic parenthetical passes the refresh diff, the golden check and coverage | tooling | data tripwire + tooling (Python side, handed to the lead) | unit + negative control |
| W-156 | Replay files and games.ndjson records carry no data, rules or code version | reporting | Rust fix (diagnostics, output bytes unchanged) | unit + e2e |
| W-157 | The card database silently loads with no overrides when overrides.json is missing | data | Rust fix (fail fast) | unit |
| W-158 | A damage-prevention threshold that is not a literal integer is silently read as unlimited prevention | engine | Rust fix (fail at registry build) + data tripwire | unit + negative control |
| W-159 | Turn-history conditions match their filters against a card's current state, not its state when the event happened | engine | port-preserve + data tripwire | unit + negative control |
| W-160 | The Python-vs-Rust equivalence harness compares results.json only up to JSON formatting, skips decision logs and the fast preset, and can reuse a stale Python run | tooling | tooling | unit + negative control |
| W-161 | ToDeck can offer duplicate ARRANGE options when a targeted Unit's Pilot is also a target | engine | port-preserve + data tripwire | unit |
| W-162 | Trigger and frame fields that are serialised but never read (host_seq, optional, answer, Battle.blocked) may hide last-known-information bugs | engine | port-preserve + fuzz diagnostic | unit + fuzz |
| W-163 | The Burst acceptance score assumes that no Burst should be declined, which was checked only on the example and test decks | ai-policy | AI (strengthened search) + data tripwire | unit + negative control |
| W-164 | Token-definition fallbacks and two override-loader forms are never exercised by current data | data | Rust fix (implemented with synthetic-data tests) + data tripwire | unit + negative control |

## AI search

### W-01 — Two-turn rollout horizon: plans that pay off later rest on the evaluation
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:111-115`, `docs/AI.md:443-452`, `docs/AI.md:366-368`, `docs/AI.md:509`, `src/gcg_sim/ai/config.py:30`, `src/gcg_sim/ai/mcts.py:119`, `README.md:167-169`, `.claude/skills/gcg-benchmark/SKILL.md:114-117`, `bench/__pycache__/weakness.cpython-315.pyc`, `81a45d9`, `23ddd49`
- **Quotes:** “Rollouts stop at the start of the turn after the reply turn; plans that pay off later rely on the evaluation.”; “A three-turn horizon costs about 43% more per iteration and is deferred for `strong`.”; “Decks that need long, precise lines may be underrated.”; “horizon_turns: int = 2”
- **What is weak:** rollouts cover the rest of the current turn plus the opponent's reply turn and stop at the next turn's first main-phase decision; anything that pays off later is priced only by the linear evaluation (the calibrated `value_scale` and the role can-kill/race terms partly compensate). Measured: a three-turn horizon costs about 43% more per iteration; on the older engine a three-turn horizon at 2x budget won 51/96. The deferred proposal is "a three-turn horizon for `strong`"; the README warns that decks needing long, precise lines may be underrated (category WK8 "long precise lines" of the lost weakness benchmark).
- **Resolution plan:** in the strengthened search only (the reference keeps `horizon_turns = 2`): (1) horizon as a preset parameter, with `standard` (budget from reference `strong`) at 3 turn boundaries and `fast` at 2; rollout decisions are counted in the node budget so the extra cost is explicit; (2) adaptive horizon: an iteration whose horizon leaf is "unsettled" (either side's race estimate at most 2 turns, or a pending battle/effect) plays one more turn boundary, decided only from the determinized world so it stays information-set honest; (3) the leaf constants generalised as in W-02; (4) the evaluation refitted at the new leaf point (W-34), because leaves then sit one turn later.
- **Verification** (scenario + ablation): scenario "two-turn plan": testkit position where the only winning line deploys a Link Unit and keeps a Blocker this turn so that a lethal attack exists on the searcher's turn after next (beyond the two-boundary horizon), while an immediate trade looks better at the reference's leaf; 16 agent seeds, reference `standard` must fail at least 8 (recorded), strengthened `standard` must solve at least 14. Ablation rows: horizon 3 alone, adaptive extension alone, both, and both with the refit evaluation; each reports nodes per game.
- **Status:** open

### W-02 — Leaf-value constants cap the horizon at four turns
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/config.py:9-10`, `src/gcg_sim/ai/config.py:48-53`, `src/gcg_sim/ai/mcts.py:254-264`, `docs/AI.md:116-119`, `docs/PROGRESS.md:158`, `d9d41af`
- **Quotes:** “horizon_turns={self.horizon_turns} is too long”; “SearchConfig rejects a horizon at which a win at the horizon would not outrank every evaluated position (WIN_DECAY * horizon_turns < LEAF_FLOOR)”
- **What is weak:** `WIN_DECAY = 0.01` and `LEAF_FLOOR = 0.05` force `WIN_DECAY * horizon_turns < LEAF_FLOOR`, so at most 4 turns; terminal values are shaded 0.01 per turn. Any deeper horizon or extension (W-01, W-03) must re-derive the constants or a certain win stops outranking every estimate.
- **Resolution plan:** the strengthened search takes the leaf mapping as parameters: estimate `floor + (1 - 2 floor)·σ(score/scale)` and terminal value `1 - decay·turns`, with `decay = floor / (max_horizon + max_extension + 1)` computed per preset, and keeps the invariant as a constructor check. The reference keeps the Python constants and its check unchanged.
- **Verification** (unit): property test over horizons 1..8 and extensions 0..2: for every strengthened config, the worst terminal win (longest line) exceeds the best possible estimate and the best terminal loss is below the worst estimate; reference config with `horizon_turns = 5` is still rejected with the Python message.
- **Status:** open

### W-03 — Lethal-threat defence: the audited "keep the last Blocker" position is unsolved; threat-aware rollouts were measured null and removed
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:443-452`, `docs/PROGRESS.md:146-149`, `docs/ASSUMPTIONS.md:92`, `tests/ai/test_puzzles.py:5`, `bench/__pycache__/weakness.cpython-315.pyc`, `d9d41af`
- **Quotes:** “it won 449/800 against 448/800 without it on the same deals, and on 100 decisions where it changed the move the old move did better in play-outs (36 to 23), so it was removed”; “with the Blocker guard and the lethal extension the AI keeps its last Blocker (8/8 seeds; baseline and guard alone trade it 8/8)”; “Remove the threat-aware rollout settings (lethal extension, face-first push, Blocker guard): measured, they do not help.”
- **What is weak:** `lethal_extension_turns` (triggers can_kill/threat/opponent), `rollout_push` (face attacks score LETHAL/2) and `rollout_guard` were built and measured: 449/800 vs 448/800 (seed 36004, 56:55 discordant), changed decisions favoured the old move 36:23 (p = 0.12). They fixed one audited defence position (blue-white-federation vs purple-white-tekkadan, match 1, decision 93) where the baseline trades its last Blocker 8/8; that puzzle left the tree with the settings, so the shipped presets presumably still trade it (WK2 "lethal avoidance").
- **Resolution plan:** do not reinstate rollout-wide threat settings. Strengthened search adds a threat-aware leaf: when the horizon leaf is the opponent's turn start (or its next attack step), an exact bounded solver of the opponent's attack step on the determinized board (engine-correct eligible Blockers, High-Maneuver, Shields and Base HP, AP per attacker, CANT_ATTACK; the opponent's hand from the determinized world) replaces the estimate with the time-shaped loss value when the opponent has lethal in that world. Solver work is counted in nodes. The same solver backs W-17 and W-38.
- **Verification** (scenario + ablation): rebuild the defence puzzle from the audit seeds (90031–90033, five BO1 games per example pairing): regenerate the game deterministically, freeze decision 93 of the bwf-pwt match 1 as a Rust scenario (or rebuild it with the testkit as d9d41af did); 8 agent seeds, reference `standard` trades the Blocker 8/8 (recorded), strengthened `standard` keeps it at least 7/8. Ablation rows: threat-aware leaf alone and leave-one-out from the bundle; adoption needs more than the null 449/448 result.
- **Status:** open

### W-04 — Budget plateau: strength is model-limited past about 2x budget; `strong` is barely stronger than `standard`
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/AI.md:329-343`, `docs/AI.md:358-368`, `docs/AI.md:479-485`, `docs/AI.md:509`, `docs/PROGRESS.md:104-106`, `docs/PROGRESS.md:110-112`, `docs/status.json:41`, `src/gcg_sim/ai/config.py:98-109`, `d9d41af`, `81a45d9`
- **Quotes:** “`strong` keeps twice `standard`'s and is only slightly stronger (0.523 above).”; “every root rule tried (sequential halving, progressive widening, near-tie extensions, visit minimums, budget-dependent exploration) was null within ±0.0004 at equal iterations.”; “209/400 = 0.523 [0.474, 0.571] (pre-change 230/400 = 0.575)”; “new standard 472/800 (0.590, [0.556, 0.624]); the same settings at the old budget 435/800 (0.544); the old AI at twice the budget 433/800 (0.541)”
- **What is weak:** decision regret against a much larger search is 0.0035 at half budget, 0.0019 at the old budget, 0.0013 at 2x and 0.0006 at 4x; past 2x the limit is rollouts, horizon and evaluation, not root allocation. `strong` vs `standard` 209/400 (before the change 230/400); the old knobs at 4x iterations scored 46/96. Better search and more iterations each gave about 4 points (the presets doubled `standard`'s iterations and traded throughput for it). Deferred: "a larger `strong` budget relative to the new `standard`".
- **Resolution plan:** strengthened `standard` starts from reference `strong` budgets and spends nodes on the model, not the root rule: the horizon (W-01), threat-aware leaf (W-03), evaluation refit (W-34, W-37), calibrated policy (W-23) and tree reuse/transpositions (W-11). Add root-parallel deterministic search: K independent trees with seeds `derive_seed(seed, "tree", k)` over disjoint world sets, merged by summed visits in fixed tree order, so the result is identical for any thread count; K and the per-tree budget are chosen on the budget curve. gcg-eval `budget-curve` measures strength at 0.5x, 1x, 2x and 4x of the starting budget to pick each preset's node budget at the knee.
- **Verification** (ablation + unit): ablation rows: budget curve of the strengthened search vs reference `strong` (at equal nodes and at 2x), root-parallel K = 1/4/8 at equal total nodes, and the full bundle; acceptance: strengthened `standard` beats reference `strong` with Wilson lower bound above 0.5 on a held-out seed run once. Unit test: root-parallel decision and statistics are bit-identical with 1, 4 and 10 threads.
- **Status:** open

### W-05 — Strategy fusion: opponent nodes pool statistics across hidden worlds (single-observer ISMCTS)
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:105-110`, `docs/AI.md:462-469`, `docs/AI.md:512-513`, `src/gcg_sim/ai/mcts.py:1-18`, `src/gcg_sim/ai/mcts.py:233-245`, `d9d41af`
- **Quotes:** “Opponent nodes pool statistics across worlds where the opponent holds different cards (0.84–0.89 distinct hands per visit)”; “Information-set MCTS with determinizations (single-observer ISMCTS).”; “auto-pass splits on 183 census positions: 389 in 24 positions, now 0”
- **What is weak:** one tree from the decider's view; `observation_keys` removed the auto-pass splits (389 in 24 of 183 positions to 0) but 12 splits of other causes remain; opponent nodes average over worlds in which the opponent holds different cards (0.84–0.89 distinct hands per visit). Measured fusion effect at opponent nodes: +0.021 at a large budget. A multiple-observer search is deferred because opponent nodes would get almost no visits at these budgets.
- **Resolution plan:** strengthened search (budgets of reference `strong` and up): partial multiple-observer opponent nodes. Opponent node statistics are split by a coarse, deterministic abstraction of the opponent's determinized private state (at most 4 buckets: holds a castable action-step trick or not, holds a Blocker-relevant card or not), so the opponent's reply depends on what it could hold without starving visits; a bucket's own statistics are used for selection only once it has at least `m` visits, and below that the node's pooled statistics are used. `m` is a preset parameter. It starts at 8 and is fixed once by a sweep over {4, 8, 16, 32} on development seeds: one ablation row per value, the largest paired difference wins, and ties go to the smaller value. The sweep rows and the chosen `m` are recorded. Combined with belief weighting (W-07). The 12 remaining splits are enumerated by a census tool in gcg-eval (the port of the 183-position census). Each split gets an observation key that removes it. If no key can be built, the split needs a decision-level ablation with the W-43 regret tool instead: with and without the split, on every census position where it occurs. The 95% interval of the regret difference must lie within ±0.0004 per decision, the bar at which the root rules of W-04 were called null. A split that meets neither stays open in this entry.
- **Verification** (scenario + ablation): scenario: the opponent holds an AP−3 action-step Command in half of the worlds; attacking into it loses the attacker, holding back is correct; 16 seeds, reference `standard` attacks in at least 8 (recorded), strengthened avoids it in at least 13. Ablation rows: bucketed opponent nodes alone and leave-one-out, plus the `m` sweep. The census reports, for each of the 12 splits, the observation key that removes it or its no-effect ablation interval, and 0 splits with neither. The Rust port of `tests/ai/test_information_set.py` (permutation invariance plus the negative control) must pass for both strengthened presets.
- **Status:** open

### W-06 — Observation keys count one child twice when the opponent passes twice in a row
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:469-474`, `src/gcg_sim/ai/mcts.py:1-18`, `src/gcg_sim/ai/mcts.py:233-245`, `d9d41af`
- **Quotes:** “both passes map to one child, which that iteration then counts twice (9 of 11,402 iterations in instrumented `fast` self-play, all on the searcher's turn)”; “the exact condition of the observation-key double count”
- **What is weak:** when the opponent passes in a battle's action step and again in the end phase's action step of the same turn with no multi-move decision between them, both passes map to one child and that iteration updates it twice (9 of 11,402 iterations).
- **Resolution plan:** strengthened search backpropagates through a per-iteration visited set: each node is updated at most once per iteration. The reference keeps the Python behaviour (it must replay bit for bit).
- **Verification** (unit + ablation): unit test with a scripted line (battle action-step pass, end-phase pass, no multi-move decision between): the shared node's visit count grows by 1 per iteration in the strengthened search and by 2 in the reference (pinned). Included in the bundle's leave-one-out row.
- **Status:** open

### W-07 — Determinization samples hidden cards uniformly: no inference from the opponent's actions
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/engine/observe.py:1-6`, `src/gcg_sim/engine/observe.py:56-83`, `src/gcg_sim/ai/evaluation.py:3-6`
- **Quotes:** “Everything else is drawn from the multiset of that owner's decklist minus”
- **What is weak:** hidden hands, Shields and deck order are drawn uniformly from the decklist minus known cards, whatever the opponent has done (passing with Resources open, holding cards, declining blocks or Bursts).
- **Resolution plan:** belief-weighted determinization for the strengthened search: each candidate world gets a weight equal to the likelihood of the opponent's last N public decisions under the opponent model (softmax of the policy scores at each recorded decision, replayed on the candidate world), and worlds are drawn by deterministic systematic resampling with the decision seed. Only public history and the searcher's own knowledge enter, so information-set honesty holds. The engine's `determinize` stays unchanged for the reference.
- **Verification** (scenario + ablation + unit): scenario: the opponent kept 2 Resources open and passed its main phase with 2 cards in hand, consistent with holding an action-step trick; 16 seeds, reference attacks into the trick in at least 8 (recorded), strengthened avoids it in at least 12. Unit tests: weights unchanged when a hidden card of the true state is changed (honesty), and permutation invariance as in W-05. Ablation row alone and leave-one-out.
- **Status:** open

### W-08 — Determinization pads or trims pools when effects move cards between players' decks
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + Rust fix (diagnostic)
- **Sources:** `src/gcg_sim/engine/observe.py:98-110`
- **Quotes:** “Pools match the slot count unless effects moved cards between players' decks; pad by”
- **What is weak:** `_fit` pads by repetition or trims deterministically, so a sampled world can hold card multisets the real game cannot contain; it only raises when the pool is empty.
- **Resolution plan:** the Rust engine keeps `_fit` bit-exact (the reference depends on it) and counts every pad/trim in a diagnostic counter. The strengthened search uses an exact pool: hidden-card multisets computed by conservation from both decklists plus every move the observer saw (owner and controller tracked separately), never padded; a conservation failure is an engine error.
- **Verification** (unit + fuzz): testkit scenario that moves a card into the other player's deck; assert the strengthened world's hidden multiset equals the true one and the reference pads (pinned). Fuzz invariant over 10,000 random games: per definition, the strengthened determinization's card counts equal the true state's.
- **Status:** open

### W-09 — Move canonicalization never pays with an EX Resource on purpose
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:61-64`, `docs/AI.md:154`, `src/gcg_sim/ai/actions.py:41-45`, `src/gcg_sim/ai/actions.py:57-85`, `src/gcg_sim/ai/policy.py:42`, `docs/CONFLICTS.md:4894`, `docs/CONFLICTS.md:4916`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:249-272`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:24`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:52`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:100-113`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:115-125`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:446`, `bench/__pycache__/positions.cpython-315.pyc`
- **Quotes:** “the representative spends the fewest EX Resources, which are single-use”; “spend EX Resources only when needed”; “EX_PENALTY = 1.5”
- **What is weak:** `action_key` keeps only the play variant (`c // ALT_PLAY_BASE`) and `canonical` keeps the fewest-EX action, so plays differing only in EX count share one key and the search only sees the cheapest; rollouts also penalise EX use. Cards that reward EX payment can therefore never be played that way when a normal payment exists: GD04-020/GD04-021 (`ex_used >= 1`), GD04-085, GD04-106/GD04-108 (`PLAYED_WITH_EX`), GD05-118 (`wp_gd05_b_played_with_ex`) and the GD05-018 EX-exile reading. Not acknowledged in the docs; the lost benchmark had an `ex_variant` puzzle.
- **Resolution plan:** strengthened search derives an EX-sensitivity table from the effect IR at registry build (programs reading `EventAmount("ex_used")`, `EventFlag("ex_used")`, the `played_with_ex` conditions, EX-exile triggers, including friendly listeners such as GD04-085); for EX-sensitive plays the move key keeps one representative per distinct EX count, and the policy drops the EX penalty when the reward condition can be met. Other plays keep the fewest-EX representative. The reference is unchanged.
- **Verification** (scenario + unit + ablation): scenario with GD04-020 (draw when a Dawn of Fold Command is played using an EX Resource) where paying with EX is clearly better: reference never pays with EX (0/16, recorded), strengthened at least 14/16. Unit test: the derived EX-sensitive set contains every card whose IR reads those conditions (cross-checked against a scan of all programs). Ablation row (bundle only; few decks hold these cards).
- **Status:** open

### W-10 — Equivalent-move merge fingerprint is conservative and depends on instance numbers
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:64-74`, `src/gcg_sim/ai/config.py:72`, `d9d41af`
- **Quotes:** “whether copies merge can depend on their instance numbers, but two different cards are never merged.”; “the merge fingerprint's conservative number checks”
- **What is weak:** `merge_equivalent_moves` compares raw numbers: a once-per-turn record or event value that merely equals a card's uid keeps two interchangeable cards apart, so visits are split unnecessarily; a less conservative port would risk merging moves that are not equivalent.
- **Resolution plan:** in Rust, history events, once-per-turn records and lasting effects carry typed card references (a `CardRef` newtype, distinct from plain integers), so the strengthened fingerprint rewrites only typed references to "self" and never confuses an integer with a uid. The reference port keeps the raw comparison for bit-exact replay.
- **Verification** (unit + property): unit test: two identical copies where an unrelated event value equals one copy's uid are kept apart by the reference (pinned) and merged by the strengthened search; property test over states from 1,000 random games: any two moves the strengthened search merges lead to successor states with equal digests after swapping the two uids.
- **Status:** open

### W-11 — Pruning and merging run only at the root; no transposition or tree reuse
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/mcts.py:89-110`, `src/gcg_sim/ai/mcts.py:220-235`, `docs/AI.md:64-66`
- **Quotes:** “At the root, attacks, blocks, choices and pairings that differ only in which of two”
- **What is weak:** `prune_dominated`, `without_effect` and `merge_equivalent` apply only at the root; below it the tree splits visits between equivalent or dominated moves, and every decision starts a fresh tree.
- **Resolution plan:** strengthened search: (1) in-tree application of the cheap, honest filters for the mover (equivalent-move merge, free-Burst decline, idle 0-AP attacks; not `without_effect`, which is too costly); (2) transposition table within a search keyed by a stable hash of the mover's observation plus the move key; (3) subtree reuse across consecutive decisions of the same player when the observed history since the last decision matches. Hashes come from `derive_seed`-style stable hashing, never process hashing.
- **Verification** (ablation + unit): ablation rows for in-tree filters, transpositions and subtree reuse at equal node budgets; regret per decision with the W-43 tool. Unit test: identical decisions and statistics across runs, thread counts and processes.
- **Status:** open

### W-12 — Root pruning is judged in one fixed determinization by the deterministic policy, with silent caps
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:75-96`, `docs/PROGRESS.md:66-68`, `docs/PROGRESS.md:113-116`, `src/gcg_sim/ai/actions.py:136`, `src/gcg_sim/ai/actions.py:139-150`, `src/gcg_sim/ai/actions.py:214-232`, `src/gcg_sim/ai/noop.py:1-18`, `src/gcg_sim/ai/noop.py:37`, `src/gcg_sim/ai/noop.py:42-52`, `src/gcg_sim/ai/noop.py:72`, `src/gcg_sim/ai/noop.py:77-96`, `src/gcg_sim/ai/noop.py:84`, `src/gcg_sim/ai/noop.py:101-103`, `src/gcg_sim/ai/noop.py:115`, `src/gcg_sim/ai/noop.py:150-151`, `src/gcg_sim/ai/mcts.py:96-107`, `src/gcg_sim/engine/interp.py:1141-1149`, `15ae2eb`, `32e9f84`, `ef05730`, `13fdeff`, `d9d41af`
- **Quotes:** “in one determinization, the move and "end the main phase / pass" are each followed by the deterministic policy to the next turn”; “_TRIAL_WORLD_SEED = 0”; “MAX_DECISIONS = 400”; “correctness bugs in root pruning (no-effect check plays only the policy's target; 0-AP attack prune accepts triggers of other cards and unaffordable tricks; Rest on already-rested targets 27 per 100 games)”; “(c.uid, c.zone, c.rested, c.damage, c.pair, c.known)”
- **What is weak:** `_drop_idle_attacks` and `_resting_changes_more_than_the_unit` trial-apply moves in one seed-0 world (`_TRIAL_WORLD_SEED`). `noop.without_effect` judges every spending move in one determinization too, but that world is seeded from the search RNG (`rng.next_u64()`, one world per decision), not from seed 0. It plays to the next turn, branches only the first SELECT, and compares a fixed signature (zones, rest, damage, pair, known bits, lasting effects, deck order). Its resolution, continuation and replay stop silently at 400 policy moves, and cut lines are compared as if complete. A move whose value appears later, or only in other worlds, can be pruned, and the dominated-move list is hand-coded. A look or reveal effect is never judged without effect, because `_h_look_top` sets `known` bits, which the signature compares. So the pruning risk lies with effects whose result depends on hidden cards but leaves no trace in the signature. Earlier bugs (policy-target-only no-op check, 0-AP prune accepting other cards' triggers and unaffordable tricks, cost-only activations) are fixed behind settings, and `baseline-standard` keeps the buggy behaviour on purpose. The policy-line baseline of the branch check is W-141.
- **Resolution plan:** strengthened search: prune only when the move is a no-op in all of the first K search worlds (K = 4, reusing the common-random-number worlds, so no extra determinization); branch every SELECT in the resolution up to a node cap counted in the budget; a cap hit or a disagreement between worlds keeps the move (fail-safe) with a prior penalty instead of removing it. The reference keeps its single worlds (seed 0 for the attack and Rest-target trials, the search-RNG world for `without_effect`), the first-SELECT branching and the 400 caps.
- **Verification** (scenario + unit + ablation): scenario "hidden-dependent effect". The searcher's spending move matters only in worlds where a hidden card is present, and in the other worlds it leaves no zone, known-bit or lasting trace by the next turn. Example: a "during this turn" protection or stat change on the attacker, which is the searcher's only Unit. It matters only when the Shield the attack destroys holds a damage 【Burst】 such as GD01-111 ("Choose 1 enemy Unit. Deal 2 damage to it."). The opponent's decklist is built so that about half of the worlds hold such a Shield. Before the scenario is used, the builder runs the reference no-effect check over 100 world seeds and confirms that the prune decision differs between worlds. Over 16 agent seeds, the test records how often the reference prunes the move (expected: in about half the seeds, wherever its one world lacks the Burst). The strengthened search must keep it in at least 14/16. Unit test: a forced cap hit keeps the move (strengthened) and is compared as complete (reference, pinned). Ablation row alone and leave-one-out.
- **Status:** open

### W-13 — The "while rested" Rest-target exception was added after the measurements and never fired at game level
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/AI.md:82-86`, `docs/AI.md:343-347`, `src/gcg_sim/ai/actions.py:212-232`, `d9d41af`
- **Quotes:** “the fix (5a6f226) adds a check that never fired in 32 games on the test and example decks, which hold no such card”; “Found by the final review (Freedom Gundam GD03-070 made the presets miss a lethal); no card in the test, example or tuning decks has such an effect, and over 32 games on them the new check never fired”
- **What is weak:** the no-op Rest-target prune ignored "while this Unit is rested" effects, so Freedom Gundam GD03-070 made the presets miss a lethal. The fix compares the derived view before and after resting each candidate in one determinization; it never fired in 32 games and the strength numbers predate it.
- **Resolution plan:** the Rust reference reproduces the fixed check bit-exactly. The strengthened search judges the rest-target prune over K worlds (W-12). gcg-eval gets an "effect coverage" deck set containing GD03-070 and every other card whose text reads its own rested state, and the instrumented trace (W-21) counts how often each prune exception fires.
- **Verification** (scenario + e2e): scenario reproducing the GD03-070 missed lethal: reference and strengthened presets both find it 16/16, and a negative control with the exception disabled misses it (shows the scenario discriminates). e2e: 100 gcg-eval games on the coverage decks report a non-zero firing count and a 400-game no-harm guard.
- **Status:** open

### W-14 — Close target choices stay noisy: the <Repair> ping puzzle is solved about a third of the time
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:122-129`, `docs/AI.md:222-224`, `docs/AI.md:383-389`, `docs/AI.md:486-496`, `docs/PROGRESS.md:63`, `tests/ai/test_puzzles_shipped.py:1-3`, `tests/ai/test_puzzles_targets.py:89-116`, `src/gcg_sim/ai/config.py:74`, `bench/__pycache__/positions.cpython-315.pyc`, `d9d41af`, `13fdeff`, `23ddd49:docs/AI.md`
- **Quotes:** “Choosing the less useful of two targets is still possible when their values stay within noise after the extension.”; “only about a third of the time by the shipped `standard` (7 of 24 and 62 of 168 agent seeds in two reviews)”; “screen at 3.0 over 72 games: regret -0.00090 +/- 0.00023 per target decision, 32 better / 6 worse, +9.5% iterations per game”; “target choices have no "do nothing" option to compare against.”
- **What is weak:** `target_extension` (up to 3 budgets while within 2 paired SE) cut regret by 0.00090 ± 0.00023 per target decision at +9.5% iterations, but close targets can still be mis-picked. The <Repair> ping puzzle (ping the Unit that keeps the damage) is solved by the shipped `standard` 7/24 and 62/168, against about 20/24 with `value_scale` 2: the calibrated leaf brings the two targets closer. It is excluded from `tests/ai/test_puzzles_shipped.py`. Not adopted: policy tie-break (worse), fixed target multiplier (−0.00036 ± 0.00019), separate root prior weight (z −1.4 and −1.0). Before #5, target choices had no "do nothing" baseline.
- **Resolution plan:** strengthened search: (1) target critic: for each candidate target, the effect is resolved to its end in each of the first K iteration worlds and scored by the evaluation; the paired score difference seeds the root prior and breaks ties only when the search's own difference is within noise; (2) variance reduction at target decisions: several rollouts per world (counted in nodes) instead of more worlds; (3) a "no target" baseline where the effect allows choosing none.
- **Verification** (scenario + ablation): the Rust port of the <Repair> ping puzzle (`tests/ai/test_puzzles_targets.py:89-116`) with 24 seeds: reference shipped `standard` at most 12/24 (recorded, about 7/24 expected), strengthened `standard` at least 20/24; the puzzle joins the shipped-preset puzzle suite for the strengthened presets. Ablation rows for the critic, multi-rollout and both, with nodes per game.
- **Status:** open

### W-15 — Undocumented rollout cap of 250 decisions: capped rollouts are evaluated mid-turn
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `src/gcg_sim/ai/config.py:33`, `src/gcg_sim/ai/mcts.py:211-214`, `src/gcg_sim/ai/playout.py:12-26`, `src/gcg_sim/ai/baseline.py:31`
- **Quotes:** “max_rollout_decisions: int = 250”
- **What is weak:** `play_until_turn` stops silently at the cap and the static evaluation then scores the state wherever it is (possibly mid-battle or mid-resolution); the cap is not in `docs/AI.md` and its frequency was never measured.
- **Resolution plan:** gcg-eval instrumentation counts capped rollouts per decision for both searches (trace only). The strengthened search continues a capped rollout with the deterministic policy to the next main-phase decision for at most a further fixed number of decisions and, if still unsettled, scores it 0.5 instead of a mid-resolution estimate. The Rust docs for the strengthened presets document the cap.
- **Verification** (unit + e2e): unit test with a tiny cap: the trace records the cap hit and the strengthened leaf is taken at a main-phase decision; e2e: capped-rollout rate per preset on 100 gcg-eval games is reported; if it is non-zero, an ablation row decides the fallback.
- **Status:** open

### W-16 — Budget is fixed by option count; only effect-target decisions may extend
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/config.py:55-57`, `src/gcg_sim/ai/config.py:80-111`, `src/gcg_sim/ai/mcts.py:121-150`, `src/gcg_sim/ai/mcts.py:172-202`
- **Quotes:** “wanted = self.iterations_per_option * n_options”; “return max(self.min_iterations, min(self.max_iterations, wanted))”
- **What is weak:** budget is `clamp(iterations_per_option·n, min, max)` (fast 7n in [16, 64], standard 64n in [160, 640], strong 128n in [320, 1280]); only target decisions extend (up to 3 budgets at z = 2); early stop when the leader cannot be overtaken. Critical decisions (lethal on board, last Blocker) get no more nodes than routine ones; generic near-tie extensions were null at equal iterations.
- **Resolution plan:** strengthened search: critical-position extension: MAIN, ATTACK, BLOCK and ACTION_STEP decisions where the threat solver (W-03/W-17) reports lethal for either side within the horizon get up to 2 extra budgets while the top two moves are within z paired SE; non-critical decisions keep the base budget. Deterministic and counted in nodes.
- **Verification** (scenario + ablation): scenario: the tactical lethal puzzles at a reduced budget (reference `fast`-sized) where the reference fails at least half of 16 seeds, strengthened `fast` solves at least 14. Ablation row with nodes per game, alone and leave-one-out.
- **Status:** open

### W-17 — Remaining implausible plays: passivity, questionable attacks, missed lethals; the audit sample is small
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/AI.md:391-398`, `docs/AI.md:441`, `docs/PROGRESS.md:107-108`, `docs/PROGRESS.md:159-163`, `docs/status.json:42-46`, `scripts/blind_transcripts.py:1-17`, `bench/__pycache__/weakness.cpython-315.pyc`, `bench/__pycache__/positions.cpython-315.pyc`, `bench/__pycache__/probes.cpython-315.pyc`
- **Quotes:** “passivity (5 → 6), questionable attacks (3 → 4) and two missed lethals in the new games show what remains.”; “The 2026-10 audit used five BO1 games per example pairing for each AI version”; “category of the weaknesses listed in docs/AI.md”
- **What is weak:** blinded audit: 19 confirmed implausible plays in 15 new-standard games (22 for the old AI): passivity 6, questionable attacks 4, wasted effects 4, bad targets 2, plus two missed lethals; five BO1 games per pairing cannot show a significant change, and the classified list lived in the gitignored `build/aiq2/audit/classified.md`. The lost weakness benchmark grouped weaknesses as WK1 lethal detection, WK2 lethal avoidance, WK3 attack/block, WK4 trick waste, WK5 resource choices, WK6 mulligan, WK7 target choices, WK8 long precise lines.
- **Resolution plan:** (1) strengthened search: exact this-turn lethal solver at the root (bounded DFS over deploy, pair, removal and attack orders with engine-correct blockers; lethal must hold in every one of the first K worlds), which plays a found lethal or gives it a dominant prior; (2) gcg-eval `weakness` suite: frozen scenarios per WK category (including the lost `hold_up`, `ex_variant`, `rest_the_active_unit`, `damage_the_unit_that_cannot_repair`, `burst_aware_attack_order`, `pair_the_linking_pilot`), scored for reference and strengthened presets; (3) a larger blinded audit (20 BO1 games per example pairing) with transcripts from a Rust port of `scripts/blind_transcripts.py`, raw outputs committed under the eval data directory; (4) recover the missed-lethal positions. Their only record was the gitignored `build/aiq2/audit/classified.md`, which is absent along with the generator `build/aiq2/audit/gen.sh` (W-127), and `docs/AI.md:397` gives only the count. gcg-eval regenerates the 15 new-`standard` audit games: reference `standard` at d9d41af, BO1, five games per example pairing, seeds 90031–90033, with decision logs. The command and the seed-to-pairing mapping are reconstructions, so they are recorded with the regenerated files. The exact lethal solver of step (1) then runs at every main-phase and attack decision of these games, from the decider's information (lethal in every one of the first K worlds). Each decision where such a lethal exists and the reference's line does not win that turn becomes a frozen scenario.
- **Verification** (scenario + ablation): one scenario per WK category, each recording the reference's failure rate and asserting the strengthened bar (16 seeds). Every missed-lethal scenario recovered by step (4) must be found 16/16 by strengthened `standard`. The number recovered is reported next to the audit's two, and is not asserted to be two, because the regenerated games cannot be checked against the lost ones. If none is recovered, the entry says so, and the lethal bar rests on the W-16 lethal puzzles. Ablation row for the lethal solver alone and leave-one-out; the audit's confirmed-implausible count per game is reported with a Wilson interval.
- **Status:** open

### W-18 — `fast` rates are not `standard` rates; fidelity rests on one weakened deck
- **Category:** ai-search · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/AI.md:400-440`, `.claude/skills/gcg-benchmark/SKILL.md:39-41`, `.claude/skills/gcg-benchmark/SKILL.md:88-97`, `tests/ai/test_fast_preset.py:1-42`, `docs/status.json:12`, `docs/status.json:47-53`, `d9d41af`
- **Quotes:** “`fast` rates are not `standard` rates”; “fast preset tests: its budget is at most 1/4.5 of standard's at every decision width and it differs from standard only in budget and noise”
- **What is weak:** on red-green-zeon vs blue-white-federation the fast-minus-standard paired difference is +0.068 [+0.011, +0.124]; fast vs standard 156/400, vs greedy 251/400; clamp(6n, 16, 64) was rejected; screening fidelity rests on one deliberately weakened deck (141/200 vs 139/200).
- **Resolution plan:** strengthened `fast` starts from reference `standard` budgets (about 10x the old `fast`), which the Rust engine makes affordable. gcg-eval `fidelity`: for each example pairing plus at least three weakened variants across colours, run `fast` and `standard` on the same deals and report paired differences and the rank correlation of variant win rates; the screening funnel keeps "confirm survivors with `standard`".
- **Verification** (e2e): fidelity report on held-out seeds: wherever strengthened `standard` resolves a matchup, strengthened `fast` favours the same deck, and every paired difference interval includes 0 or is below 0.05 in magnitude; Rust slow-suite port of `tests/ai/test_fast_preset.py` (budget ratio and settings parity).
- **Status:** open

### W-19 — Strength acceptance bars are thin floors
- **Category:** ai-search · **Resolution type:** eval tooling (acceptance gates)
- **Sources:** `docs/AI.md:351-361`, `docs/SPEC.md:58`, `docs/status.json:12`, `docs/status.json:47-53`, `tests/ai/test_puzzles.py:22`, `tests/ai/test_puzzles_shipped.py:25`, `tests/ai/test_puzzles_targets.py:89-92`, `tests/slow/test_ai_strength.py:48`, `tests/slow/test_ai_strength.py:56`
- **Quotes:** “these results are floors, not the measure of improvement.”; “SEEDS = (1, 2)”; “292/400 vs greedy (Wilson 95% [0.684, 0.771])”
- **What is weak:** criterion 7a needs only at least 90% against random and a Wilson lower bound above 0.5 against greedy (measured 400/400 and 292/400 = 0.730); greedy shares the role weights; tactical puzzles use two agent seeds and the target puzzle requires 16/24.
- **Resolution plan:** the Rust reference keeps criterion 7a unchanged (equivalence). The strengthened presets get their own gates in gcg-eval: strengthened `standard` vs reference `strong` and strengthened `fast` vs reference `standard`, each Wilson lower bound above 0.5 on a held-out seed run once; every tactical puzzle with 8 agent seeds and a per-puzzle bar recorded with the scenario.
- **Verification** (e2e): Rust slow tests for both sets of gates; a gate below its bar fails the suite.
- **Status:** open

### W-20 — Strength evidence rests on few decks, and the tuning decks overlap the example decks
- **Category:** ai-search · **Resolution type:** eval tooling (deck panel)
- **Sources:** `docs/AI.md:228-231`, `docs/AI.md:329-342`, `docs/AI.md:351-353`, `src/gcg_sim/ai/tuning.py:64-93`, `tests/ai/decks.py:14`, `tests/ai/decks.py:38`, `tests/ai/decks.py:71`, `examples/decks/red-green-zeon.txt`, `examples/decks/purple-white-tekkadan.txt`, `examples/decks/blue-white-federation.txt`, `tests/bench/__pycache__/test_preset_evidence.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “132 of 240 = 0.550 [0.487, 0.612]”; “The tuning decks are different from the AI test decks.”; “DECKS: tuple[DeckList, DeckList] = (FEDERATION_DECK, SEED_DECK)”
- **What is weak:** the headline 925/1600 = 0.578 is on two test decks, and the example-deck check is not significant (45, 46, 41 of 80 per pairing). Overlaps were verified this session by multiset intersection of main-deck copies. Tuning decks against example decks: tuning Zeon shares 27/50 with red-green-zeon, tuning Tekkadan 29/50 with purple-white-tekkadan, and tuning Wing 2/50 with blue-white-federation. Test decks against example decks: test deck 0 (Blue/White Federation, `FEDERATION_DECK`) shares 27/50 with blue-white-federation and 3/50 with purple-white-tekkadan. Test deck 1 (Red/White SEED, `SEED_DECK`) shares 3/50 with blue-white-federation and 2/50 with red-green-zeon. So the example-deck spot check (132/240) is independent of neither the tuning decks nor the test decks. The artifacts sweep attributed the tuning finding to a "PD20" note in the cited bytecode; that string is not in the file, so the claim rests on the recomputation.
- **Resolution plan:** gcg-eval deck panel: a fixed, versioned set of legal decks (curated lists plus a seeded generator over colour pairs), split into tuning and held-out halves. No held-out deck may share more than 10/50 main-deck copies with any tuning deck or any AI test deck. All strengthened-vs-reference evidence is reported per panel deck and pooled with a stratified interval. The example decks are reported separately and labelled by their overlap with the tuning and test decks, never as held-out evidence.
- **Verification** (unit + e2e): a unit test computes the overlaps and reproduces, on the current files, 27/50, 29/50 and 2/50 (tuning against example decks) and 27/50, 3/50, 3/50 and 2/50 (test against example decks). It enforces the panel bound against both deck sets. e2e: panel head-to-head report.
- **Status:** open

### W-21 — The decision log keeps the top 5 alternatives and does not record search effort
- **Category:** ai-search · **Resolution type:** eval tooling (instrumentation)
- **Sources:** `src/gcg_sim/ai/base.py:15`, `src/gcg_sim/ai/base.py:84`, `bench/legacy_ai/__pycache__/instrumented.cpython-315.pyc`
- **Quotes:** “LOG_ALTERNATIVES = 5”; “The decision log keeps only the top alternatives”
- **What is weak:** the log cannot show how many iterations a decision used, extension rounds, early stops or capped rollouts, so equal-cost comparisons needed a separate instrumented agent.
- **Resolution plan:** keep the Python decision-log format byte-identical (equivalence). gcg-eval writes a separate per-decision trace (all root moves with visits and means, nodes used, early stop, extension rounds, capped rollouts, prune exceptions fired) to its own file, never to `games.ndjson` or replays.
- **Verification** (unit + e2e): unit test: trace node counts sum to the nodes the search used and list every root move; e2e: `games.ndjson` and replays are byte-identical with tracing on and off.
- **Status:** open

## AI policy

### W-22 — Greedy baseline: one determinization, baseline knobs, fixed choices, silent cap
- **Category:** ai-policy · **Resolution type:** port-preserve + eval tooling
- **Sources:** `docs/AI.md:8`, `docs/AI.md:79-81`, `docs/AI.md:149-150`, `docs/ASSUMPTIONS.md:88`, `docs/ASSUMPTIONS.md:94`, `src/gcg_sim/ai/policy.py:50-60`, `src/gcg_sim/ai/agents.py:18-19`, `src/gcg_sim/ai/agents.py:35-66`
- **Quotes:** “greedy keeps the older, looser test”; “greedy always uses them”; “GREEDY_MAX_DECISIONS = 200”
- **What is weak:** one-ply look-ahead on one determinization with `BASELINE_KNOBS` and `DEFAULT_WEIGHTS` (now the role weights, so it plays differently than before); the turn completion stops at 200 decisions and capped lines are scored mid-turn; it always goes first and uses 48 mulligan samples. Results against it are floors.
- **Resolution plan:** greedy stays a frozen reference baseline (criterion 7a depends on it) and is ported bit-exact; improvement claims are made against reference presets (W-19), not greedy. gcg-eval counts greedy cap hits in its trace.
- **Verification** (e2e + unit): equivalence: Rust greedy vs Python greedy on fixed seeds give identical games and decision logs; unit test that a forced cap hit is counted.
- **Status:** open

### W-23 — Hand-written playout policy: Commands under-cast, abilities over-activated, unfitted constants
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:145-163`, `docs/AI.md:453-461`, `src/gcg_sim/ai/policy.py:1-8`, `src/gcg_sim/ai/policy.py:42-47`, `src/gcg_sim/ai/policy.py:175-183`, `src/gcg_sim/ai/policy.py:187-250`, `src/gcg_sim/ai/policy.py:288-298`, `src/gcg_sim/ai/config.py:67`, `src/gcg_sim/ai/config.py:75-76`, `d9d41af`, `81a45d9`, `23ddd49:docs/AI.md`
- **Quotes:** “Rollouts and priors come from a hand-written policy.”; “s = 2.5 + 0.3 * V.cdef(st, o.a).cost - cost”; “effect-aware Command scoring and priors distilled from search statistics made decisions worse in the investigation and were not built.”; “Scores every legal option of any decision from the current board without look-ahead”
- **What is weak:** measured: rollouts cast Commands in 33% of turns where search casts them in 45%, and activate abilities 55% of the time against search's 22% (the pre-#5 docs said the opposite, "plays Commands somewhat eagerly"). Scores are hand-set: `EX_PENALTY = 1.5`, `LETHAL = 60`, `CLEAR_THE_WAY = 8`, `MUST_BLOCK = 100`, `KEEPABLE_TOP = 2`, `IDLE_ATTACK = -1`, PLAY_COMMAND `2.5 + 0.3·cost` (+8 for removal when blocked from lethal), ACTIVATE flat 1.5 (Support 0.5). Effect-aware Command scoring and priors distilled from search statistics made decisions worse and were not built; the Burst, 0-AP face-attack and rule-polarity fixes shipped.
- **Resolution plan:** strengthened presets use a frequency-calibrated policy: a handful of scalar offsets (PLAY_COMMAND base, ACTIVATE base, Support, end-phase pass preference, EX penalty) fitted by deterministic coordinate search so that rollout action frequencies match the strengthened search's own choice frequencies on its self-play games (moment matching on 4–6 scalars, not a distilled prior over all features, which was measured worse). The fitted table is a constant of each strengthened preset; the reference keeps the Python constants.
- **Verification** (ablation + e2e): gcg-eval reports the rollout-vs-search frequency gap for Commands and activations on the deck panel (target: within 5 points each); ablation rows: calibrated policy alone and leave-one-out.
- **Status:** open

### W-24 — Modal choices are scored flat
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:455-456`, `src/gcg_sim/ai/policy.py:349-351`, `d9d41af`
- **Quotes:** “modal choices are scored flat”
- **What is weak:** when the decision is a mode choice every option scores 0, so rollouts pick modes uniformly (up to temperature) and priors for mode choices are uniform.
- **Resolution plan:** strengthened policy scores each mode by a one-ply look-ahead: resolve the mode on a clone of the rollout world (nested choices by the deterministic policy) and score the evaluation difference; results cached per (program, mode, state version); cost counted in nodes.
- **Verification** (scenario + unit + ablation): unit test: on a modal Command with one useless mode, reference rollout mode frequencies are about 50/50 (recorded), strengthened picks the useful mode at least 90% at rollout temperature. Scenario where the search's value of holding vs casting a modal Command depends on rollouts choosing the right mode later; 16 seeds, strengthened at least 13, reference recorded. Ablation row.
- **Status:** open

### W-25 — The opponent is modelled with the searcher's own policy
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:456-457`, `docs/AI.md:512-513`, `src/gcg_sim/ai/mcts.py:205-217`
- **Quotes:** “the opponent is modelled with the same policy”; “A multiple-observer search, or opponent models other than the shared policy, if a larger budget makes opponent nodes worth splitting.”
- **What is weak:** rollouts for both seats call the same policy with the same knobs, so the opponent is exactly as passive or eager as the searcher's own heuristic; other opponent models are deferred.
- **Resolution plan:** opponent modelling in the strengthened search: (1) separate opponent rollout knobs (temperature and the calibrated offsets of W-23) chosen by ablation, including a lower-temperature "competent opponent"; (2) optional per-deck opponent profile: the W-23 calibration run on reference self-play of the benchmark deck, stored per deck digest and used only when the opponent's decklist matches; (3) belief-weighted worlds (W-07) use the same opponent model. All deterministic.
- **Verification** (scenario + ablation): scenario "over-extension": a line that loses to a punishing reply the shared policy plays only rarely; 16 seeds, reference over-extends in at least 8 (recorded), strengthened at most 3. Ablation rows per opponent setting and leave-one-out.
- **Status:** open

### W-26 — Kill-aware and damage-payoff targeting were measured and removed
- **Category:** ai-policy · **Resolution type:** AI (strengthened search, re-measure)
- **Sources:** `docs/AI.md:457-461`, `docs/ASSUMPTIONS.md:92`, `docs/PROGRESS.md:143-145`, `d9d41af`
- **Quotes:** “made no difference to the search's choices (+0.00025 ± 0.00032 against a reseed placebo), so they were removed”; “kill-aware and damaged-payoff targeting were measured and dropped (squashed from ws/ai-policy)”
- **What is weak:** the policy's own Damage picks improved (regret −0.020 per Damage decision) but the search's choices did not change, so the settings were removed; rollouts keep picking Damage targets without regard to kills or follow-up payoffs.
- **Resolution plan:** re-implement both as strengthened-policy options and re-measure under the strengthened search, where the target critic (W-14), longer horizon (W-01) and calibrated policy (W-23) change what rollouts feed back; adopt only under the adoption rule, otherwise record the null row and leave them out again.
- **Verification** (ablation): regret per Damage decision with the W-43 tool (policy-level and search-level) and an ablation row alone and leave-one-out.
- **Status:** open

### W-27 — YES_NO answers say no about 12% of the time; the policy always goes first and keeps
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:161-163`, `src/gcg_sim/ai/policy.py:305-318`, `src/gcg_sim/ai/agents.py:55-56`
- **Quotes:** “Yes (score 1 against 0, so a rollout at temperature 0.5 says no about 12% of the time).”; “return [1.0 if o.kind is A.YES else 0.0 for o in dec.options]”
- **What is weak:** Bursts were fixed (`policy_burst_yes_score` 4.0, 0.03% declines; W-163 covers the assumption behind that fix) but generic YES_NO was not, and "always yes" may be wrong for optional costs. CHOOSE_FIRST always picks itself and REDRAW always keeps; greedy always goes first.
- **Resolution plan:** strengthened policy scores YES_NO by the one-ply look-ahead of W-24: a free beneficial "you may" gets a Burst-like high score, an optional cost is accepted only when the look-ahead says it pays. Pre-game choices of search agents use the setup procedures (W-44, W-45); policy and greedy stay as in the reference.
- **Verification** (unit + ablation): unit tests on rollout distributions: free beneficial YES_NO accepted at least 99.9%; a harmful optional cost (e.g. a "you may discard 1" whose payoff is idle) declined at least 90% (reference 88% yes, pinned). Ablation row.
- **Status:** open

### W-28 — End-phase action-step plays are under-explored by rollouts
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:157`, `docs/AI.md:375-376`
- **Quotes:** “Tricks during a battle are tried; passing is preferred in the end phase.”
- **What is weak:** rollouts prefer passing in the end-phase action step, so lines that cast removal or rest effects there (the puzzle where the agent casts removal in the opponent's end step so the Blocker stays rested) are found only by tree search, not by rollouts.
- **Resolution plan:** strengthened policy scores end-phase action-step plays with the one-ply look-ahead (W-24) instead of a flat pass preference, limited to plays whose effect outlasts the turn (rest, destroy, damage, lasting effects) so rollouts do not waste tricks.
- **Verification** (scenario + ablation): the "hold removal" puzzle at a reduced budget (reference `fast`-sized): reference solves at most 8/16 (recorded), strengthened `fast` at least 14/16. Ablation row.
- **Status:** open

### W-29 — No policy scorer for ORDER_TRIGGER decisions
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/policy.py:104-108`, `src/gcg_sim/ai/policy.py:388-400`, `src/gcg_sim/engine/types.py:107`
- **Quotes:** “if scorer is None:”; “ORDER_TRIGGER = "order_trigger"”
- **What is weak:** trigger ordering has no scorer: greedy and deterministic rollouts take the first option, softmax rollouts are uniform, and search priors for these decisions are uniform.
- **Resolution plan:** strengthened policy: ORDER_TRIGGER scored by look-ahead (resolve each order with the deterministic policy and evaluate) when there are at most 6 orders, otherwise a heuristic (own beneficial triggers first, draw before discard, damage before Repair on the same Unit).
- **Verification** (scenario + unit): scenario with two simultaneous triggers whose order matters (draw-then-discard vs discard-then-draw): reference picks option order (pinned), strengthened picks the better order in at least 15/16 seeds; unit test for the heuristic ordering.
- **Status:** open

### W-30 — Target polarity classification is coarse
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/targeting.py:41-42`, `src/gcg_sim/ai/targeting.py:51`, `src/gcg_sim/ai/targeting.py:58-61`, `src/gcg_sim/ai/targeting.py:126-137`, `src/gcg_sim/ai/targeting.py:148-160`, `tests/ai/test_policy_knobs.py:96-105`
- **Quotes:** “otherwise every "can't" rule”
- **What is weak:** without the rule table (greedy and the baseline) every `cant_` rule counts as harmful and every other rule as helpful, and the test lists sites the prefix rule signs wrongly; even with the table, CUSTOM rules, unsigned PLAY_COST/LEVEL_DELTA and StatMods with zero or non-integer totals are UNKNOWN, and only the first harmful, helpful or Apply step on the chosen variable is read.
- **Resolution plan:** strengthened policy: an exhaustive polarity table in Rust (a `match` over every rule kind, so a new kind fails to compile until signed), CUSTOM rules signed per custom id, PLAY_COST/LEVEL_DELTA signed by the amount's sign, non-integer StatMods resolved by look-ahead; all steps acting on the chosen variable are combined.
- **Verification** (unit + ablation): unit test: no rule kind or custom id in the registry classifies as UNKNOWN; the mis-signed sites listed in `tests/ai/test_policy_knobs.py:96-105` classify correctly. Ablation row.
- **Status:** open

### W-31 — Heuristic card and fight values are rough
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/values.py:1`, `src/gcg_sim/ai/values.py:13-47`, `src/gcg_sim/ai/values.py:65-71`, `src/gcg_sim/ai/values.py:83-87`
- **Quotes:** “Rough worth of a Unit or Base on the field, in "stat points".”
- **What is weak:** a flat `KEYWORD_BONUS` of 0.3 per keyword whatever the keyword; `card_worth` uses only type, cost and stats and ignores text and board; `fight()` handles only AP against remaining HP and First Strike (no damage prevention or reduction, redirect, Breach, triggers or tricks); life counts Base HP / 3. Shared by the policy and the setup choices.
- **Resolution plan:** strengthened policy: fight outcomes come from the engine's own battle damage resolution on a clone (exact prevention, reduction, First Strike and Breach), and keyword worth comes from the per-keyword evaluation features fitted in W-37 instead of a flat bonus.
- **Verification** (unit + ablation): unit test: strengthened fight outcomes equal engine battle results on a corpus of attacker/defender pairs including damage-reduction and First Strike cards (reference `fight()` disagreements recorded); ablation row.
- **Status:** open

### W-32 — AI Blocker counts ignore CANT_BLOCK and NO_BLOCKER_VS
- **Category:** ai-policy · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/values.py:74-80`, `src/gcg_sim/ai/evaluation.py:173-176`, `src/gcg_sim/ai/evaluation.py:249-253`, `src/gcg_sim/engine/battle.py:46-75`, `docs/port/inventory/08-ai-policy-eval.md:1319-1322`
- **Quotes:** “and not V.rules_of(dv, u, d.RuleKind.CANT_BLOCK)”; “Active Units of ``player`` with <Blocker> (candidates to intercept an attack).”; “`_race` ignores rested Blockers.”
- **What is weak:** `ready_blockers` (used by the policy's board and block-risk scoring) and the evaluation's per-side Blocker count take every active <Blocker>. The engine's `eligible_blockers` also excludes CANT_BLOCK and the attacker's NO_BLOCKER_VS rules. `_race` checks CANT_BLOCK but not NO_BLOCKER_VS, and it counts every defender Blocker, rested or not, while `_side.blockers` and `ready_blockers` count only active ones (verified in the code this session). The port inventory flags this asymmetry as undocumented. It may be deliberate, since the race assumes the boards stay as they are and the defender's Units stand up.
- **Resolution plan:** strengthened policy and evaluation use one `effective_blockers(attacker)` helper that calls the engine's eligibility rules. The race (W-38) asks the same helper which Blockers are active on each simulated turn. The reference keeps its counts, including `_race`'s rested Blockers.
- **Verification** (unit + scenario): unit test: a Unit under CANT_BLOCK, an attacker with NO_BLOCKER_VS, and (for the race) a rested Blocker are excluded by the strengthened helper and counted by the reference (pinned). Scenario: lethal is on because the only enemy Blocker cannot block. The reference search sees the engine's real block options in its tree and may already find the lethal, so the test records the reference `standard` search's solve count over 16 seeds, not only what the reference policy does. The scenario counts as a reproducer only if the reference solves at most 8/16, and the strengthened `standard` must solve 16/16. If the reference already solves it, W-32 rests on the unit test and the W-38 ablation row, and the scenario stays as a no-regression check.
- **Status:** open

### W-33 — AI module-level caches are unbounded and not invalidated on script override
- **Category:** ai-policy · **Resolution type:** Rust fix (no behaviour change)
- **Sources:** `src/gcg_sim/ai/targeting.py:54-55`, `src/gcg_sim/ai/targeting.py:93-112`, `src/gcg_sim/ai/actions.py:167-199`, `src/gcg_sim/effects/registry.py:209-215`, `src/gcg_sim/testkit.py:212`
- **Quotes:** “_harm_cache: dict[int, bool] = {}”
- **What is weak:** `_harm_cache` is keyed by `def_id`; `override_script` (tests) changes a card's program but only the listen cache is cleared, so a stale removal classification is possible in tests; the `(program_id, pc)` caches are safe because programs are append-only but are process-global mutable state.
- **Resolution plan:** in Rust these caches belong to the registry (or a per-search context) and carry the registry generation; an override bumps the generation. No process-global mutable caches.
- **Verification** (unit): override a card's Command program in a test registry and assert the harm classification follows the new program; both searches' outputs unchanged on the equivalence seeds.
- **Status:** open

## AI evaluation

### W-34 — Evaluation fitted on pre-change search games; refit and per-preset fits deferred
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:165-183`, `docs/AI.md:218-219`, `docs/AI.md:226-256`, `docs/AI.md:475-478`, `docs/AI.md:507-508`, `docs/ASSUMPTIONS.md:92`, `docs/PROGRESS.md:143-145`, `src/gcg_sim/ai/evaluation.py:1-13`, `src/gcg_sim/ai/evaluation.py:104-129`, `d9d41af`
- **Quotes:** “refitting to the new search's own games is a follow-up.”; “test whether `strong` and `fast` benefit from their own fits.”; “a refit of the 14 base weights on the same data was worse out of fold (+0.0025) and did not ship.”; “ROLE_WEIGHTS: the roles feature set fitted on 540 standard-preset self-play games on the tuning decks (7,941 positions; CV log-loss 0.0090 below TUNED_WEIGHTS”
- **What is weak:** `ROLE_WEIGHTS` (the default for every preset) come from self-play of the pre-change `standard` search with `TUNED_WEIGHTS` (seed 35001, 540 games, 7,941 positions; L2 λ = 1, Newton's method) but serve the new presets; all presets share them. A standard-budget refit of the 14 base weights was worse out of fold and did not ship (`TUNED_WEIGHTS` stays for `baseline-standard`).
- **Resolution plan:** Rust tuning tool in gcg-eval (port of `tuning.py`, same fit, made deterministic and worker-count independent) refits the role features on strengthened `standard` self-play at the strengthened leaf point, and separately for strengthened `fast`; iterate fit → self-play → refit until held-out log-loss stops improving (at most 3 rounds). Weights become constants of the strengthened presets; the reference keeps `ROLE_WEIGHTS` and `TUNED_WEIGHTS`.
- **Verification** (ablation + e2e): log-loss table (out of fold by game, test decks, held-out panel decks of W-20) must improve on `ROLE_WEIGHTS`; ablation rows per preset (refit vs `ROLE_WEIGHTS`) alone and leave-one-out.
- **Status:** open

### W-35 — The evaluation is linear; interaction terms and other candidate features gained nothing
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:208-212`, `docs/AI.md:219-221`, `docs/AI.md:475-477`, `src/gcg_sim/ai/evaluation.py:303-307`
- **Quotes:** “Interaction terms (life × enemy threat), a sharper lethal estimate, a held-trick feature and dropping `bursts` were measured and gained nothing (within ±0.001).”; “**Linear evaluation.** The model is still linear.”
- **What is weak:** one logistic model for every phase of the game; base-set features that did not help were dropped (Unit count, Blocker count, EX Resources, deck size, exposed flag, maximum AP, trash size, a continuous attack margin); interaction terms gained nothing. The role features replaced the forced antisymmetry, the one measured misspecification.
- **Resolution plan:** strengthened evaluation: phase-conditioned linear model (separate weight vectors for early, mid and late game, binned by turn and level, sharing the role features), fitted with the W-34 tool; the exact threat solver (W-38) replaces the approximate lethal terms. Evaluation sums run in a fixed order in f64. Fixed order alone does not make results identical across platforms, because the logistic leaf calls `exp` (`evaluation.py:303-307`) and platform libm results can differ (W-137). So the strengthened evaluation calls the port's portable `exp` and `ln`, never the platform libm. W-137 names that implementation: safe Rust in `gcg-core` that uses only IEEE-754 basic operations. The reference keeps libm and is pinned on the golden platform only.
- **Verification** (ablation + unit): out-of-fold log-loss against the single linear refit; ablation row; unit test that strengthened evaluation and leaf values match recorded vectors bit for bit on two platforms (CI matrix: macOS arm64 and Linux x86-64).
- **Status:** open

### W-36 — The evaluation was trained on three tuning decks and transfers weakly to other decks
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `docs/AI.md:213-218`, `docs/AI.md:228-231`, `docs/AI.md:329-332`
- **Quotes:** “+0.0030 (SE 0.003, slightly worse) on example-deck games”
- **What is weak:** tuning decks are Zeon green/red, Wing green/blue and Tekkadan purple; role weights gain −0.0090 nats out of fold and −0.0074 on test decks but are slightly worse (+0.0030) on example decks; at the tuning budget a role agent scored 384/800 = 0.480 [0.446, 0.515] against a base refit.
- **Resolution plan:** the W-34 refit trains on the tuning half of the deck panel (W-20) and reports per-deck held-out log-loss; a per-deck regression gate.
- **Verification** (e2e): per-deck held-out log-loss table: no example or held-out panel deck worse than `ROLE_WEIGHTS` by more than one standard error.
- **Status:** open

### W-37 — The evaluation ignores most keywords, card text and effects
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:185-200`, `.claude/skills/gcg-refresh-data/references/engine-extension.md:21`, `src/gcg_sim/ai/evaluation.py:30-45`, `src/gcg_sim/ai/evaluation.py:132-140`, `src/gcg_sim/ai/evaluation.py:159-202`, `src/gcg_sim/ai/evaluation.py:246-269`
- **Quotes:** “give the keyword a value in the evaluation features, so search does not ignore it”; “hand[_HAND_SLOT.get(cd.card_type, 3)] += 1”
- **What is weak:** features are life, stats, linked and ready counts, hand counts by type, level and burst density; the only keywords read are Blocker and High-Maneuver/can't-be-blocked; Breach, Repair, Support, First Strike, Suppression and printed abilities have no feature; unknown card types fall into the Base hand slot; burst density is one fraction over all unseen cards. Each new keyword needs a manual evaluation update.
- **Resolution plan:** strengthened evaluation adds per-keyword amount features on the battlefield (one per `Kw` variant, derived through an exhaustive `match`, so a new keyword cannot be forgotten), counts of Units with activatable and triggered abilities from the IR, an explicit hand slot per card type (unknown type is an error), and per-Shield Burst probability. Fitted with W-34.
- **Verification** (unit + ablation): unit test: every `Kw` variant maps to a feature; log-loss improvement; ablation row.
- **Status:** open

### W-38 — can_kill and race features are crude board heuristics
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:174-180`, `docs/AI.md:190`, `docs/PROGRESS.md:113-114`, `src/gcg_sim/ai/evaluation.py:59`, `src/gcg_sim/ai/evaluation.py:173-176`, `src/gcg_sim/ai/evaluation.py:205-212`, `src/gcg_sim/ai/evaluation.py:244-277`, `docs/port/inventory/08-ai-policy-eval.md:1319-1322`
- **Quotes:** “can_kill leaves (10.8%) are converted by rollouts only 46% of the time”; “castable removal in hand”; “capped at :data:`RACE_CAP`”; “It counts the defender's Blockers whether rested or not”
- **What is weak:** can_kill counts all ready Units (all Units for the non-active side) as attackers whatever their AP, CANT_ATTACK or summoning state, counts the Base as one hit whatever its HP, and lets any castable removal answer one Blocker. The race assumes every Unit with AP attacks each turn, each Blocker stops the largest blockable attack and the Base absorbs hits at the mean connecting AP, with no development, removal or recovery, capped at 6 turns. It counts every defender Blocker, rested ones included (`evaluation.py:249-253`), while the per-side Blocker feature counts only active ones (`evaluation.py:173-176`). This asymmetry is undocumented (W-32).
- **Resolution plan:** strengthened evaluation replaces can_kill with the exact bounded lethal solver (W-17/W-03) on the determinized board. It computes the race with engine-correct attackers and blockers (W-32), tracking which Blockers are active on each simulated turn, and the race cap becomes a fitted feature transform. The reference keeps `_race` as written, rested Blockers included.
- **Verification** (unit + ablation): unit test on 10,000 positions from random and search games: the strengthened can_kill agrees with exhaustive search of attack lines (reference disagreement rate recorded). A second unit test uses a position with a rested defender Blocker: the reference race counts it (pinned), and the strengthened race does not count it on a turn when it is still rested. Log-loss and ablation row.
- **Status:** open

### W-39 — The negative tempo weight is an artifact of when positions are recorded
- **Category:** ai-evaluation · **Resolution type:** port-preserve + AI (strengthened search)
- **Sources:** `docs/AI.md:187`, `docs/AI.md:202-204`
- **Quotes:** “The negative `tempo` weight reflects when positions are recorded”
- **What is weak:** the base set's `tempo` weight is −0.20 because positions are recorded after the turn player's draw and resource placement, which hand and level already count; the roles set uses an intercept instead.
- **Resolution plan:** the reference keeps `TUNED_WEIGHTS` for `baseline-standard` bit-exactly. Strengthened feature sets have no tempo feature (intercept only), and the Rust tuning tool records and reports the exact recording point so the fit and the leaf point cannot drift apart.
- **Verification** (unit): unit test: strengthened feature sets contain no tempo feature, and the tuning tool's recording point equals the search's leaf point (both asserted from the same constant).
- **Status:** open

### W-40 — Weight-tuning methodology limits
- **Category:** ai-evaluation · **Resolution type:** eval tooling
- **Sources:** `src/gcg_sim/ai/tuning.py:1-16`, `src/gcg_sim/ai/tuning.py:50-62`, `src/gcg_sim/ai/tuning.py:64-96`, `src/gcg_sim/ai/tuning.py:148-162`, `src/gcg_sim/ai/tuning.py:184-213`, `src/gcg_sim/ai/tuning.py:280-289`, `src/gcg_sim/ai/tuning.py:341`, `src/gcg_sim/ai/tuning.py:348-364`, `src/gcg_sim/ai/tuning.py:395-400`, `src/gcg_sim/ai/compare.py:4-5`, `docs/AI.md:252-253`, `ef05730`
- **Quotes:** “went on to win (draws are dropped)”; “Challenger's score in one game (1 win, 0.5 draw, 0 loss).”; “wins-only strength metric”
- **What is weak:** three decks (three of nine matchups are mirrors); the label is the game outcome at one position per turn; correlated positions; even/odd game split; the final weights are fitted on all positions without a held-out check; λ fixed at 1 with 30 Newton steps in pure Python; weights rounded to 4 decimals; self-play uses the same weights on both sides; validation always plays against `HAND_WEIGHTS` (tuned 112 vs hand 104, Wilson [0.452, 0.584]), scores draws as 0.5 (inconsistent with the wins-only metric) and reports no interval.
- **Resolution plan:** the gcg-eval `tune` subcommand: game-clustered k-fold cross-validation, λ chosen by CV, final weights checked on a held-out deck set, validation against the current default weights with the wins-only metric, Wilson and paired intervals (W-42), the panel decks of W-20, tuning budget equal to the target preset's, deterministic and independent of worker count.
- **Verification** (unit): same seed gives bit-identical weights with 1, 4 and 10 workers; no CV fold splits a game; validation counts draws as non-wins; a synthetic dataset with known weights is recovered within tolerance.
- **Status:** open

### W-41 — Matchup estimates are not calibrated against real tournament results
- **Category:** ai-evaluation · **Resolution type:** eval tooling (data from the user)
- **Sources:** `docs/AI.md:506`, `docs/ASSUMPTIONS.md:88-89`
- **Quotes:** “Calibrate matchup estimates against real tournament results (needs data from the user).”; “No real-world win rates were fetched or invented”
- **What is weak:** strength is measured only by head-to-head play, puzzles, the audit and regret; no real results exist in the repo, and calibrating needs data only the user can supply.
- **Resolution plan:** gcg-eval `calibrate`: reads a user-supplied file of observed matches (two decklists, wins, losses, draws, source, date), simulates each matchup with a chosen preset, and reports simulated vs observed rates with intervals, Brier score, log-loss and reliability bins. Nothing is fetched or invented; the tool ships with a schema and a synthetic fixture. Supplying real data is a question for the user, raised by the lead.
- **Verification** (e2e): synthetic fixture whose "observed" rates equal simulated rates gives zero calibration error; a fixture with a known bias reports it; schema validation rejects malformed input.
- **Status:** open

### W-42 — No paired analysis of same-seed runs; compare reports a pooled Wilson interval
- **Category:** reporting · **Resolution type:** eval tooling
- **Sources:** `docs/AI.md:516-517`, `.claude/skills/gcg-benchmark/SKILL.md:104-110`, `src/gcg_sim/ai/compare.py:1-5`, `src/gcg_sim/ai/compare.py:11-12`, `src/gcg_sim/ai/compare.py:146-178`, `src/gcg_sim/ai/selfplay.py:88-110`, `bench/__pycache__/weakness.cpython-315.pyc`, `bench/__pycache__/stats.cpython-315.pyc`
- **Quotes:** “Paired analysis of same-seed variant runs in the benchmark skill (fewer games for the same precision).”; “the win rate carries a Wilson 95% interval”; “game_seed = derive_seed(master_seed, "block", block, "decks", swap_decks)”
- **What is weak:** a mirrored block of four holds two deals, not one. `mirrored_specs` derives the game seed from the block and the deck order (`swap_decks`), and each deal is played twice with the agents swapped between seats, so the challenger plays both decks and both seats within each deal. The `mirrored_specs` docstring and `docs/AI.md:326-327` say one deal per block of four (W-129). The games of a deal share their cards, yet `compare.py` treats all games as independent and gives no paired statistic. Variants also share per-decision seeds through the config name. The benchmark skill compares card swaps by unpaired Wilson intervals ("if the intervals overlap heavily"). The lost harness used paired Newcombe and block-bootstrap intervals.
- **Resolution plan:** gcg-eval paired statistics for every head-to-head and ablation row: per-deal paired differences (each deal is the pair of games with the same game seed, played with the agents swapped), Newcombe interval for paired proportions, McNemar discordant counts and a seeded bootstrap over deals. Also a `compare-runs` command that pairs two same-seed `gcg-sim benchmark` result directories game by game (additive output; the reports themselves are unchanged).
- **Verification** (unit + e2e): unit tests against published Newcombe test vectors. Unit test: the pairing groups exactly the two games of each block that share a game seed, giving two pairs per block of four. e2e: two identical same-seed runs give a zero difference with no discordant pairs, and a known-better variant gives a narrower paired interval than the unpaired one.
- **Status:** open

### W-43 — Decision-level regret (oracle replay) tooling was never committed
- **Category:** tooling · **Resolution type:** eval tooling
- **Sources:** `docs/AI.md:514-515`, `docs/PROGRESS.md:109-112`, `scripts/blind_transcripts.py:1-17`
- **Quotes:** “Decision-level regret tooling (the oracle replay used in the investigation) as a committed tool for future search changes.”
- **What is weak:** every regret figure in `docs/AI.md` (target extension, budget scaling, policy targeting) came from investigation tooling that is not in the repo; `scripts/blind_transcripts.py` covers only the audit transcripts.
- **Resolution plan:** gcg-eval `regret`: sample positions from recorded games deterministically, run the candidate search and an oracle (strengthened search at 8x budget), play both chosen moves out in M shared worlds, and report regret per decision with a paired standard error by decision kind. Deterministic and worker-count independent.
- **Verification** (unit + e2e): the oracle's regret against itself is exactly 0; an agent that picks random targets shows positive target regret with an interval excluding 0; identical output for 1 and 8 workers.
- **Status:** open

## AI setup

### W-44 — Play/draw choice: policy-vs-policy simulations, biased to going first, capped simulations scored 0.5
- **Category:** ai-setup · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:131-136`, `docs/AI.md:163`, `docs/PROGRESS.md:69-70`, `src/gcg_sim/ai/setup.py:86-120`
- **Quotes:** “the agent goes second only when that is better by more than two standard errors of the paired difference”; “go second only if that is significantly better; otherwise go first.”
- **What is weak:** `first_player_games` pairs (8 fast, 24 standard, 48 strong) of policy-vs-policy games; goes second only when 2 SE better; simulations capped at 3,000 decisions, or drawn, silently score 0.5. Player One won 436/720 in MCTS self-play; policy and greedy always go first.
- **Resolution plan:** strengthened search uses its calibrated rollout policy (W-23) in the simulations, excludes capped or drawn simulations from the paired estimate instead of scoring them 0.5, and records the number excluded in the trace.
- **Verification** (unit + e2e): unit test: a capped simulation is excluded (strengthened) and scored 0.5 (reference, pinned); e2e: the trace reports go-first rates per matchup and excluded simulations.
- **Status:** open

### W-45 — Mulligan heuristic: greedy four-turn opening score with a fixed 90% threshold
- **Category:** ai-setup · **Resolution type:** AI (strengthened search)
- **Sources:** `docs/AI.md:137-143`, `docs/AI.md:163`, `src/gcg_sim/ai/setup.py:22-27`, `src/gcg_sim/ai/setup.py:30-60`, `src/gcg_sim/ai/setup.py:63-83`, `bench/__pycache__/weakness.cpython-315.pyc`
- **Quotes:** “The agent redraws when the hand scores below 90% of the average of `mulligan_samples` fresh five-card hands”; “How much board a hand develops over the first turns if played out greedily.”
- **What is weak:** `opening_score` counts only Units and Pilots over four turns with a greedy curve, ignoring Commands, Bases, effects and the matchup; `KEEP_RATIO = 0.9` is fixed; `mulligan_samples` 96 (192 for strong); policy and greedy rollouts always keep (WK6 "mulligan regret" in the lost benchmark).
- **Resolution plan:** strengthened mulligan by paired simulation, reusing the play/draw machinery: keep vs redraw compared on shared determinized opening worlds with the strengthened rollout policy; redraw only when better by 2 paired SE, otherwise fall back to the opening-score rule.
- **Verification** (scenario + ablation): scenarios: a hand of early Commands and a Base that `opening_score` undervalues, and a Unit-only hand that fails against the matchup; ground truth from 2,000 paired simulations; reference wrong (recorded), strengthened right in at least 14/16 seeds. Mulligan regret via the W-43 tool; ablation row.
- **Status:** open

## Engine

### W-46 — Safety caps: 200 turns and 20,000 decisions share one end reason; the decision cap is untested; self-play stops earlier with no end reason
- **Category:** engine · **Resolution type:** Rust fix (additive diagnostics)
- **Sources:** `README.md:165-166`, `docs/ASSUMPTIONS.md:64`, `docs/REPORTS.md:40-45`, `docs/INTERFACES.md:18`, `src/gcg_sim/engine/game.py:33`, `src/gcg_sim/engine/game.py:112-113`, `src/gcg_sim/engine/game.py:325-332`, `src/gcg_sim/engine/game.py:737-742`, `src/gcg_sim/engine/game.py:786`, `src/gcg_sim/engine/state.py:507-508`, `src/gcg_sim/engine/types.py:155`, `src/gcg_sim/runner/config.py:15`, `src/gcg_sim/runner/config.py:31`, `src/gcg_sim/runner/game.py:44`, `src/gcg_sim/runner/match.py:22-26`, `src/gcg_sim/reports/results.py:35-37`, `src/gcg_sim/ai/selfplay.py:22`, `src/gcg_sim/ai/selfplay.py:60-85`, `src/gcg_sim/ai/tuning.py:159-161`, `src/gcg_sim/ai/tuning.py:286-289`, `src/gcg_sim/tools/fuzz.py:107-110`, `tests/runner/test_match.py:158-163`
- **Quotes:** “Games that reach the engine's safety cap (200 turns or 20,000 decisions) are reported as `turn_limit` draws.”; “TURN_LIMIT = "turn_limit" # engine safety cap (reported, never silent)”; “if st.action_count > st.max_actions:”; “winner = -1 if st.winner is None else st.winner”
- **What is weak:** both caps end the game with `EndReason.TURN_LIMIT`, so reports cannot tell them apart; `action_count` grows only in `apply`; only the turn cap is tested (`tests/runner/test_match.py:158-163`). A turn-limit game scores no match point and the next chooser is re-rolled; the fuzz tool counts it as a failure. Self-play (`compare`, strength tests, tuning) stops at its own `DECISION_LIMIT = 20000`, before the engine's `> 20000` check, leaving `winner = None`: `play()` maps it to a draw, tuning drops it, `duel` scores 0.5, and no end reason is recorded.
- **Resolution plan:** the Rust engine keeps the `turn_limit` end reason in every reference-equivalent output, and records which cap fired (`CapKind::Turns` or `CapKind::Decisions`) in the game record's diagnostics (exposed in gcg-eval traces and a `--diagnostics` side file, not in `results.json` or replays). gcg-eval self-play has no loop cap of its own: it relies on the engine caps and records the end reason; capped games are listed in every head-to-head summary.
- **Verification** (unit + e2e): unit tests: `max_actions = 10` ends with `turn_limit` and `CapKind::Decisions`; `turn_limit = 2` gives `CapKind::Turns`; equivalence of reports with Python for both. e2e: a gcg-eval head-to-head with a tiny decision cap lists the capped games instead of silent draws.
- **Status:** open

### W-47 — No guard against decision-free loops inside advance()
- **Category:** engine · **Resolution type:** Rust fix (error path only)
- **Sources:** `src/gcg_sim/engine/game.py:734-760`, `src/gcg_sim/engine/game.py:786`
- **Quotes:** “while st.pending is None and st.winner is None:”
- **What is weak:** `advance()` checks only the `max_actions` cap and `action_count` grows only when a decision is applied, so a chain of triggers or frames that never asks for a decision (e.g. delayed triggers re-triggering each other) would hang. No known card does this.
- **Resolution plan:** Rust `advance()` counts engine steps since the last decision and returns `EngineError::NoProgress` past a large bound (e.g. 1,000,000 steps). Legal games never get near it, so every output is unchanged; the fuzzer treats the error as a crash.
- **Verification** (unit + fuzz): a test-only card script with two mutually re-triggering delayed triggers returns `NoProgress` within the bound; 10,000 fuzz games never hit it.
- **Status:** open

### W-48 — The derived-view fixpoint is silently capped at six passes
- **Category:** engine · **Resolution type:** Rust fix (diagnostic, identical output)
- **Sources:** `src/gcg_sim/engine/view.py:5`, `src/gcg_sim/engine/view.py:409-461`, `tests/perf/__pycache__/test_viewdeps.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “for _ in range(6):”; “GD02-076 (AP 4) gains <Blocker> while it has 5 or more AP; a lasting AP+1 is applied in”
- **What is weak:** constant and lasting effects are applied in a small fixpoint; if the view has not converged after 6 passes the last iteration is used with no error, and layers are not ordered by dependency or timestamp. The lost view-dependency tests record a case that needs a confirming pass (GD02-076 gains <Blocker> only after a lasting AP+1), and custom steps that run `Apply` through `interp.execute` are opaque to static analysis.
- **Resolution plan:** Rust keeps the same six-pass loop (identical results) and adds a convergence flag: a view that hits the cap without converging raises a diagnostic in the `paranoid` feature and in fuzz runs. Any future dependency-ordered view (W-124) must reproduce the fixpoint result, including the GD02-076 case.
- **Verification** (unit + fuzz): unit test: GD02-076 with a lasting AP+1 has AP 5 and <Blocker>; a test-only oscillating script triggers the diagnostic; 10,000 fuzz games report zero non-converged views.
- **Status:** open

### W-49 — The derived-view cache relies on manual touch() calls and a heuristic key; only the paranoid guard checks it
- **Category:** engine · **Resolution type:** Rust fix (by construction)
- **Sources:** `src/gcg_sim/engine/view.py:223-239`, `tests/paranoid.py:1-8`, `tests/conftest.py:1-2`, `docs/AI.md:500-502`, `docs/ARCHITECTURE.md:50-51`, `.claude/skills/gcg-refresh-data/references/engine-extension.md:30`, `aa0dd30`, `d9d41af`
- **Quotes:** “No view input depends on the”; “Engine cache guard: random-agent and greedy games check every cached derived view against a fresh computation; GCG_SIM_PARANOID_VIEW=1 runs the same check in every test”; “once rules management finds no damage reaching printed HP, it skips the state until the next change (st.touch() bumps the version; every write to decks, Units, Bases, damage, pairing or identities touches)”; “Derived view cache keys on turn player, turn, battles and history so no caller can read a stale view”
- **What is weak:** the cache key is (active player, turn, number of battles, last battle, history length) plus version bumps from `st.touch()`, and assumes no view input depends on phase or step; a missed `touch()` means a stale view or a missed destruction. An earlier stale-view bug was fixed in aa0dd30; several output-identical shortcuts are protected only by the opt-in `GCG_SIM_PARANOID_VIEW=1` guard.
- **Resolution plan:** in Rust, state fields are private and every mutation goes through methods that bump the version, so a missing touch cannot compile; the cache key is the version. A `paranoid` cargo feature recomputes every cached view and re-runs every skipped rules check, and runs in CI over the full test suite and the fuzz games.
- **Verification** (unit + fuzz): `cargo test --features paranoid` and 10,000 paranoid fuzz games with zero mismatches; a compile-fail test shows a direct field write outside the state module does not compile.
- **Status:** open

### W-50 — The listened-events fast path depends on the decklists; the listen cache is process-global
- **Category:** engine · **Resolution type:** Rust fix (by construction)
- **Sources:** `src/gcg_sim/engine/core.py:119-121`, `src/gcg_sim/engine/core.py:287-313`, `src/gcg_sim/engine/core.py:334`, `src/gcg_sim/testkit.py:144-145`, `src/gcg_sim/testkit.py:212-217`, `docs/ARCHITECTURE.md:50-51`, `.claude/skills/gcg-refresh-data/references/engine-extension.md:30`, `c6716f5`, `d9d41af`
- **Quotes:** “Unless it is a token, it must be in its owner's decklists”; “if len(_LISTEN_CACHE) > 256:”; “Fix test scenarios skipping the triggers of cards added after start: Scenario.add then puts the card into its owner's decklists, which the engine's listened events follow”; “Events that fire for every card (like `TURN_END`) go in its `always` set, or the fast path will skip them.”
- **What is weak:** `emit()` skips events no card in either decklist (or a token or Unit variant) can trigger on; a card created outside the decklists silently misses its triggers (test helpers `Scenario.add_raw`, `pair_raw`, `raw_hand` did). The listen cache is a process-global dict keyed by decklists and cleared wholesale above 256 entries; tests must clear it after script overrides. The `always` set is hand-maintained.
- **Resolution plan:** Rust computes the listened-event set per game at `new_game` and stores it in the game; every card-creation path (deck, token, variant, test helper) goes through one constructor that extends the set; the `always` set is derived from the IR (triggers with no subject filter). No global cache.
- **Verification** (unit + fuzz): a testkit card added after start triggers; the derived `always` set equals the hand-maintained Python set (or the difference is explained); the paranoid feature checks every skipped event has no trigger candidate over the fuzz games.
- **Status:** open

### W-51 — The rules-management HP shortcut depends on a registry-global "no HP-lowering effect" flag
- **Category:** engine · **Resolution type:** Rust fix (identical output)
- **Sources:** `src/gcg_sim/effects/registry.py:356-376`, `src/gcg_sim/engine/core.py:857-875`, `src/gcg_sim/engine/core.py:917-921`, `d9d41af`, `e82f08c`
- **Quotes:** “Without HP-lowering effects a card's HP”; “rules management skips the derived view when no Unit or Base has damage reaching its printed HP plus its Pilot's (without HP-lowering effects HP is never lower, so nothing can be destroyed)”
- **What is weak:** `hp_reduction_possible` is computed over the whole card database, so one HP-lowering card anywhere disables the shortcut in every game; test-only text that lowers HP once broke it (fixed in d9d41af by recomputing on overrides); correctness is checked only by the paranoid guard.
- **Resolution plan:** Rust computes the flag per game from the cards that can appear in it (decklists, tokens, variants) and recomputes it on test overrides; since the shortcut is output-identical by construction, results do not change.
- **Verification** (unit + fuzz): regression test of the d9d41af fix (test-only HP-lowering override leaves no lethal-damaged Unit in play); paranoid fuzz with mixed decks reports zero skipped destructions.
- **Status:** open

### W-52 — GameState.clone shares pending, the derived view and history entries (CB-1)
- **Category:** engine · **Resolution type:** Rust fix (by construction)
- **Sources:** `src/gcg_sim/engine/state.py:546`, `src/gcg_sim/engine/state.py:558`, `src/gcg_sim/engine/state.py:570`, `src/gcg_sim/engine/observe.py:86-95`, `docs/ASSUMPTIONS.md:88`, `docs/PROGRESS.md:133-135`, `d9d41af`
- **Quotes:** “s._derived = self._derived”; “Fix determinize and permute_hidden writing sampled identities into the true game's history: clone() shares HistoryEvent objects”
- **What is weak:** the shallow clone shares the Decision object, the cached derived view and the `HistoryEvent` objects; `determinize` and `permute_hidden` once wrote sampled identities into the true game's history (CB-1): play was unaffected, but final digests depended on the searches (2 of 63 games changed).
- **Resolution plan:** Rust clones have value semantics (shared parts are immutable, e.g. persistent or `Rc` vectors with copy-on-write), and determinization takes `&GameState` and returns a new state, so it cannot write into its source.
- **Verification** (unit + e2e): Rust port of `tests/ai/test_search_side_effects.py` (true-state digest unchanged by `determinize`, `permute_hidden` and a whole decision of each search); the 63-game benchmark's final digests equal agent-free replays.
- **Status:** open

### W-53 — Registry indices are process-local, so saved states do not reload faithfully in another process
- **Category:** engine · **Resolution type:** Rust fix (content-addressed ids)
- **Sources:** `docs/AI.md:510-511`, `docs/REPORTS.md:115-117`, `src/gcg_sim/runner/digest.py:1-7`, `src/gcg_sim/runner/digest.py:26-27`, `src/gcg_sim/effects/registry.py:123-138`, `src/gcg_sim/engine/interp.py:1276-1283`, `src/gcg_sim/engine/interp.py:1305`, `src/gcg_sim/engine/interp.py:1592`, `src/gcg_sim/engine/core.py:392`, `src/gcg_sim/engine/state.py:202`
- **Quotes:** “a stable, process-independent `Lasting.effect_key` so saved states reload faithfully in another process.”; “stores some registry indices that are assigned in first-use order”
- **What is weak:** `GameState.to_json` stores registry indices interned in first-use order (continuous effects, filter sets, lazily registered system and delayed programs, once-per-turn keys); frozen StatMods create a new continuous entry per distinct amount, so the registry grows. Only the digest canonicalizes them, with `repr()`-based SHA-256 content ids, which ties digests to Python `repr`. Deferred proposal.
- **Resolution plan:** Rust uses content-addressed ids (SHA-256 of a canonical serialization of the IR node) for continuous effects, filters, programs and once-per-turn keys in its saved-state format; the equivalence digest keeps Python's canonical form (computed from the same content ids through the ported `repr`, verified against exported content ids).
- **Verification** (unit + e2e): save a mid-game state, reload it in a fresh process whose registry was built in a different order, continue: final digest equals uninterrupted play; digest vectors equal Python's on the equivalence seeds.
- **Status:** open

### W-54 — Dead enum values: the PAYMENT decision kind and Step.ATTACK_TRIGGERS
- **Category:** engine · **Resolution type:** Rust fix (no behaviour change)
- **Sources:** `src/gcg_sim/engine/types.py:79`, `src/gcg_sim/engine/types.py:114`, `src/gcg_sim/engine/game.py:31-32`, `src/gcg_sim/engine/game.py:372-382`, `tests/perf/__pycache__/test_engine_dispatch.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “PAYMENT = "payment"”; “``Step.ATTACK_TRIGGERS`` is never entered”
- **What is weak:** EX-vs-normal payment is folded into the play action's `c` parameter (`ALT_PLAY_BASE`), so no code creates a PAYMENT decision and the policy has no scorer for it; `Step.ATTACK_TRIGGERS` is never entered (attack triggers resolve as a trigger batch before the attack-end check, rule 8-2-2).
- **Resolution plan:** Rust keeps both variants only for serialization compatibility (their string and integer values), marks them unreachable in the engine with a debug assertion, and documents the `c` encoding.
- **Verification** (fuzz): 10,000 fuzz games and the equivalence runs never produce a PAYMENT decision or the ATTACK_TRIGGERS step (assertion counts zero).
- **Status:** open

### W-55 — Interpretations and N/A rules that hold only for the current card pool
- **Category:** engine · **Resolution type:** port-preserve + data tripwire
- **Sources:** `docs/ASSUMPTIONS.md:54`, `docs/CONFLICTS.md:1271-1284`, `docs/CONFLICTS.md:1091-1099`, `docs/research/RULES_INDEX.md:207`, `src/gcg_sim/engine/battle.py:150-151`, `src/gcg_sim/data/rules/rules_na.json:164-166`, `src/gcg_sim/data/rules/rules_na.json:284-291`, `e911333`, `97d3e08`, `7a797fb`, `19886fe`
- **Quotes:** “Bases deal no battle damage to the attacking Unit (every Base has 0 AP).”; “Re-test if a card that places several Resources from the resource deck is ever added.”; “Nothing in the pinned card pool moves more than one card out of the resource deck at a time, so this rule never changes what the engine does.”; “Regenerate RULES_TRACEABILITY from the full JUnit run: 395 tested, 184 N/A, 0 missing”; “Accept N/A reasons for token-supply and resource-deck multi-move rules; update index tests”
- **What is weak:** several rules decisions are safe only because no current card breaks them: Bases have 0 AP and deal no battle damage back (contradicts the literal 3-5-4 and 13-1-5-2; First Strike against a Base changes nothing); rule 4-3-3 (simultaneous multi-card moves from the resource deck) is unimplemented and N/A; there is no token supply or token-count limit (5-17-4-1, 5-17-4-3). 184 rules are N/A by design, some of them pool-dependent. A future card could silently make any of these wrong.
- **Resolution plan:** the Rust engine implements the decisions exactly. A data-gate test suite ("tripwires") fails when a packaged card breaks a precondition: a Base with non-zero AP, any IR step that moves more than one card from the resource deck at once, any IR that needs a token supply, and, for every N/A reason in `rules_na.json` classified as pool-dependent, a check of its precondition. A failing tripwire blocks a data refresh until the rule is implemented.
- **Verification** (unit + negative control): tripwires pass on the packaged pool; a synthetic card in a temporary data copy (Base with AP 1; a "place 2 Resources" step) makes the matching tripwire fail.
- **Status:** open

### W-56 — Rules-interpretation decisions the port must keep, pinned by rulings and curated conflicts
- **Category:** engine · **Resolution type:** port-preserve + conformance tests
- **Sources:** `docs/ASSUMPTIONS.md:56`, `docs/ASSUMPTIONS.md:59-61`, `docs/ASSUMPTIONS.md:63`, `docs/ASSUMPTIONS.md:65-71`, `docs/ARCHITECTURE.md:88-89`, `docs/CONFLICTS.md:1286-1300`, `docs/CONFLICTS.md:1302-1312`, `docs/CONFLICTS.md:1314-1327`, `docs/CONFLICTS.md:1617`, `docs/CONFLICTS.md:1973`, `docs/CONFLICTS.md:2384`, `docs/CONFLICTS.md:2798`, `docs/CONFLICTS.md:3128`, `docs/CONFLICTS.md:4072`, `docs/CONFLICTS.md:4098-4105`, `docs/CONFLICTS.md:4216`, `docs/CONFLICTS.md:4729-4737`, `docs/CONFLICTS.md:4784-4792`, `docs/CONFLICTS.md:5389-5398`, `docs/CONFLICTS.md:5411-5423`, `docs/CONFLICTS.md:5425-5436`, `docs/CONFLICTS.md:5452-5463`, `docs/CONFLICTS.md:5465-5477`, `docs/CONFLICTS.md:5479-5490`, `docs/research/RULES_INDEX.md:201-211`, `src/gcg_sim/engine/game.py:116-117`, `src/gcg_sim/engine/interp.py:1033-1038`, `src/gcg_sim/engine/interp.py:1051-1055`, `src/gcg_sim/engine/core.py:18-19`
- **Quotes:** “Non-token cards go to the removal area (5-12); tokens leave the game (5-17-2-5).”; “A triggered effect is used up only if it performs an action (paying a cost counts).”; “A Unit with 0 AP that attacks a player with no Base and no Shields deals no damage, so the player is not defeated.”; “return False # rules 4-4-2, 4-4-2-1”; “Follow 10-2-2-1 literally: removal-area cards are not "targets"”; “Sets 1 rested Resource active, normal Resources before EX Resources, without a player choice.”
- **What is weak:** a family of recorded interpretation choices, each open to a future official ruling and each easy to break silently in a re-implementation: "exile" = removal area (tokens leave the game; an EX Resource paid as a cost counts as exiled for GD05-018); placement beyond 15 Resources / 5 EX does nothing; lasting "all your Units" effects apply only to Units present at resolution (Q105); uids assigned after the shuffle and cut; owner-ordered simultaneous placement into private zones; 【Once per Turn】 used up only when the effect acts (Q197); delayed "During this turn, when …" triggers repeat; "during this battle" effects outside a battle do nothing; destruction cause tracking (effect damage vs "destroy", Q287, Q368); modal Commands need a fully targetable mode; attack-target constraint priority (GD04-107, Q289–Q301, Q388); "deploy it as an (APx･HPy) Unit" (Q389, Q390); removal-area cards are not targets; last placed Shield is on top; a 0-AP attack on an empty-shield player does not defeat; 【When Linked】 triggers only on pairing; <Suppression> with a Base present does nothing; old <Breach> reminder wording ignored; "pay its cost to deploy it" skips the Lv. check; "If …"/"If you do" govern a following "Then" (Q175, Q233); resolutions that remove a player decision (ST14-015, GD03-129, GD03-076).
- **Resolution plan:** Rust implements each exactly as in Python (rules unchanged). Every decision gets its ASSUMPTIONS/CONFLICTS anchor as an interpretation id attached to the Rust ports of the tagged Python tests (`rule`, `ruling`, `faq`, `card` tags), and a generated traceability table maps id → Rust tests. The data-refresh gate lists every new official FAQ or ruling that cites a rule or card of one of these ids for re-review.
- **Verification** (unit + differential): all ported tagged tests pass; differential fuzz (Python vs Rust final digests) over 10,000 random games with generated decks containing the affected cards (EB01-070, GD01-058, GD01-082, GD02-105, GD02-106, GD02-119, GD04-107, GD05-089, ST14-015 and the 17 <Suppression> and 41 <Breach> cards) shows zero differences.
- **Status:** open

### W-57 — Historical engine, validator, runner and tooling bugs fixed in Python: regression risk for the port
- **Category:** engine · **Resolution type:** port-preserve + regression tests
- **Sources:** `102e72b`, `7c47b86`, `ddb9f4f`, `0e1d3e8`, `0ee591d`, `e911333`, `013ea8f`, `816f9e4`, `c8cc8e9`, `f89c0cc`, `4dd6701`, `39acfd1`, `fb51ba3`, `6283471`, `97d3e08`, `ca93dff`, `7d290d6`, `48d152f`, `f78574e`, `62af367`, `f042788`, `3e37b60`, `e16e494`, `118f324`, `db94768`, `7e52f58`, `331b70f`, `9408681`, `38aaf43`, `21489cb`, `ecdacf9`, `e24ed91`, `15ae2eb`, `ef05730`, `d9d41af`
- **Quotes:** “Fix engine/compiler bugs from rule-tagged tests: Then-scope, choose-N and grouped targets, instead substitutions, Development, play modifiers, name aliases, after-Main pairing”; “engine deck check counts printings together (2-1-1)”; “effect-begun battles see battling effects (Q447, Q455)”; “clean errors for non-UTF-8 replay files and failed report writes naming the file”; “--rounds 0 no longer mislabels the start weights”
- **What is weak:** 89 historical strict xfails were added in 26 commits and closed in 102e72b (26), 7c47b86 (22), ddb9f4f (3), 0e1d3e8 (16) and 0ee591d (22); none remain. Fixed bugs include: battle LKI and Base damage; <Breach> amount read from the destroyed host; 【Destroyed】 ignoring the last-known paired Pilot; HP filters using printed HP (Q96); <Support> bonus lost with its source (Q106); no concession entry point; "All players draw 1" defeating only the active player; `new_game` accepting non-conforming decks; phase/step labels advancing early; deck placement ordering and information leaks (4-1-6, 4-1-7, 4-2-2); "during this battle" effects never expiring outside a battle (re-found in at least 11 commits); a Unit trashed by an effect not treated as destroyed; play modifiers skipped; missing COST_PAID; Once per Turn keys and LKI zone_seq; missing command-activation events; effect-damage destruction attribution; one-shot delayed triggers; modal Commands with no legal mode; shield damage prevention and thresholds; paired Pilot destruction; forced/attracted attack targets; reused rest substitutes and shared lasting keys; HasKeyword off the field; redirected battle damage; turn-history gaps; effect-activated paired Commands; effect-begun battles; simultaneous "all players each" effects; multi-token deployments; printings counted separately in the deck check; FAQ Q1–Q6 once N/A; unclean errors for non-UTF-8 files; `tuning --rounds 0` mislabelling.
- **Resolution plan:** port every tagged Python test (rule, card, FAQ and ruling tags) with its tag; a regression table in the Rust test tree maps each historical xfail id and each fixed bug above to the Rust test that covers it; Python-vs-Rust differential replays run in CI.
- **Verification** (unit + meta-test): all ported tests pass; a meta-test fails if any historical xfail id or listed bug lacks a Rust test; differential fuzz shows zero digest differences.
- **Status:** open

## Effects

### W-58 — Most-defensible readings of ambiguous card text and major ruling overrides
- **Category:** effects · **Resolution type:** port-preserve + reporting
- **Sources:** `README.md:170-171`, `docs/CONFLICTS.md:75-90`, `docs/CONFLICTS.md:1259-1314`, `docs/CONFLICTS.md:4282-4284`, `docs/CONFLICTS.md:5334-5343`, `docs/PROGRESS.md:32-34`, `data/conflicts.json`, `da570b3`
- **Quotes:** “Where no official ruling exists, the most defensible reading was chosen and recorded there.”; “Total: 402 conflicts; unresolved: 0”; “Curate 103 card-agent readings of ambiguous text as CONFLICTS entries with verified quotes and resolutions”; “No official source settles the question”
- **What is weak:** 402 conflicts: 108 ambiguous card text (106 minor), 192 rulings vs card text (4 major), 16 FAQ vs rules; by policy, 110 `most_defensible_reading`, 192 `ruling_overrides_text`, 45 `canonical_printing`, 16 `faq_clarifies_rules`, 11 `rules_xref_intended_target`, 6 `banlist_most_defensible`. Major: errata ST12-001, markers GD02-053, rulings GD02-021:Q175, GD03-072:Q233, GD04-101:Q287, GD05-054:Q368. Behaviour on these cards (e.g. ST13-013 simultaneous 1-to-2 token deploys) may change if Bandai rules differently.
- **Resolution plan:** Rust reads `overrides.json` and the conflicts data as packaged (no re-encoding, no change); reports list every interpretation that affected a run (W-101); the refresh gate's re-review list (W-56) also covers these conflict ids.
- **Verification** (unit + e2e): data-loading test: 402 conflicts, 0 unresolved, policy counts equal `data/conflicts.json`; e2e: a run with a deck containing ST13-013 lists that conflict id.
- **Status:** open

### W-59 — Pilot text routing is inferred, not encoded in the data
- **Category:** effects · **Resolution type:** port-preserve + audit table
- **Sources:** `docs/ASSUMPTIONS.md:58`, `docs/research/DATA_INGESTION.md:280-284`, `docs/research/EFFECT_TEMPLATES.md:762-768`, `src/gcg_sim/effects/compiler/__init__.py:32-49`
- **Quotes:** “the data does not mark which lines are above the name.”; “This split is inferred from the text, not encoded in the data.”; “【Burst】 and location-specific effects stay with the Pilot card.”
- **What is weak:** gcg-api joins both Pilot texts into one effect string; 【Burst】, name-alias, play-modifier and hand/trash lines stay with the Pilot and everything else goes to the paired Unit. A misattributed line would be a silent rules bug.
- **Resolution plan:** the Rust effect layer reproduces the same split (exported IR or ported compiler); a generated audit table (card → lines kept by the Pilot vs granted to the Unit) is checked in with the Rust test data, so any change in the split, including for new Pilot cards, shows up as a reviewed diff.
- **Verification** (unit): the audit table regenerates identically from the packaged data; Rust and Python splits agree for every Pilot card.
- **Status:** open

### W-60 — Explanatory parentheticals are stripped but carry semantics the bindings must honour
- **Category:** effects · **Resolution type:** port-preserve + tests
- **Sources:** `docs/research/EFFECT_TEMPLATES.md:91-95`, `docs/research/DATA_INGESTION.md:137-141`, `.claude/skills/gcg-refresh-data/references/card-checklist.md:21-22`
- **Quotes:** “The bindings for those cards must still honour them.”; “they state intended semantics, so effect implementers should read the raw text”
- **What is weak:** reminder-text normalization (rule 2-11-4) removes "(Don't treat it as a Pilot.)" (GD05-089), "(You choose first, then your opponent.)" (ST12-013) and "(This card that is being activated is not counted.)" (ST14-014), yet the bindings must implement them.
- **Resolution plan:** a Rust test per affected card checks the parenthetical's behaviour (GD05-089 is not a Pilot while deployed as a Unit; ST12-013 choice order; ST14-014 does not count itself), and the data gate lists every stripped parenthetical that is not plain reminder text so new ones get a test. W-155 extends that gate to the parentheticals' text, so a reworded one is caught too.
- **Verification** (unit): the three behaviour tests pass in Rust and Python; the stripped-parenthetical list on the packaged data contains exactly the known cards.
- **Status:** open

### W-61 — Grammar slips and duplicated token clauses in card text break templates
- **Category:** effects · **Resolution type:** port-preserve + tests
- **Sources:** `docs/research/EFFECT_TEMPLATES.md:934-939`
- **Quotes:** “**Grammar slips that break templates:** ST09-004 ("While a friendly Base in play"),”
- **What is weak:** ST09-004 ("While a friendly Base in play"), GD02-086 ("this gets AP+1"), GD05-101 ("one of your Unit's effects"), GD04-100 ("pay ① or more cost") and tokens T-014/T-021/T-022/T-029 (duplicated definition clauses) need bindings or tolerant templates.
- **Resolution plan:** Rust keeps the same per-card behaviour; each listed card has a behaviour test, and the compiler's tolerance list (if the compiler is ported) is data-driven and tested against these exact texts.
- **Verification** (unit): behaviour tests for the eight cards pass in Rust with the same golden IR as Python.
- **Status:** open

### W-62 — 'instead' wording: conditional overrides vs true substitutions
- **Category:** effects · **Resolution type:** port-preserve + tests
- **Sources:** `docs/research/EFFECT_TEMPLATES.md:795-806`
- **Quotes:** “Compile these as a conditional override of the previous step's parameters, not as a rule-10-1-9 substitution.”
- **What is weak:** twelve "instead" texts are conditional parameter overrides of the previous step; the true 10-1-9 substitutions are GD03-079 and GD05-124 (rest substitution), GD04-087 and GD04-095 (damage redirect), ST12-012 and GD05-089. Confusing the two changes card behaviour.
- **Resolution plan:** Rust models the two as different IR nodes (override vs replacement effect) with no shared code path; each of the 18 cards has a behaviour test.
- **Verification** (unit): 18 behaviour tests pass in Rust; a test asserts every "instead" text in the pool maps to exactly one of the two node kinds.
- **Status:** open

### W-63 — The regex compiler covers about 64% of non-vanilla cards; 302 per-card bindings; new sets need manual multi-agent work
- **Category:** effects · **Resolution type:** general templates, golden-identical (Rust or Python side, after the IR decision)
- **Sources:** `docs/status.json:18-24`, `docs/research/EFFECT_TEMPLATES.md:34-42`, `docs/research/EFFECT_TEMPLATES.md:911-915`, `docs/ARCHITECTURE.md:28-29`, `docs/ARCHITECTURE.md:96-99`, `src/gcg_sim/effects/compiler/__init__.py:1-5`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:144`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:173`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:186`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:265`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:320`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:384`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:428`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:437`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:197`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:246`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:559`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:577`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:618`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:707`, `.claude/skills/gcg-refresh-data/SKILL.md:318-331`, `CLAUDE.md:44-47`, `docs/port/inventory/04-effects-ir.md:950-952`
- **Quotes:** “The last 189 cards would need 225 single-use templates, one or two per card, so per-card bindings are the cheaper route for them.”; “Any fragment it cannot recognise raises :class:`CompileError`;”; “recommend to the user that they run it with `ultracode`”; “Will the Rust engine load a Python-exported IR (recommended) or must it also compile card text”
- **What is weak:** status: 538 compiled, 302 binding, 308 vanilla; the research estimated 189 binding cards but 302 exist. About 38 binding notes say "compile error" (selector suffixes such as "paired with this Unit", delayed "during this turn, when …", inline token definitions, "when you play and activate … using an EX Resource", "are deployed rested", conditional damage reduction, pronoun "it" without antecedent, "exile this card in your trash", "It can't activate <Blocker>"). Large releases need about 65 card numbers per package of manual binding work. The project rule prefers general templates over per-card hooks.
- **Resolution plan:** first, the lead records the open port decision: does Rust load a Python-exported IR (the inventory's recommendation) or port the text compiler (`04-effects-ir.md:950-952`)? The plan then depends on the answer. (a) Exported IR: the general templates for the recurring compile-error families above are added to the Python compiler through the binding workflow (handed to the lead), and the Rust work is only the IR node types they need. (b) Ported compiler: the templates or DSL nodes are added in the Rust effect layer. Either way, the bindings they make redundant are retired, and a change is accepted only when the card's golden IR behaviour is identical (golden check empty, differential fuzz clean). Bindings stay where no general form exists, and card behaviour does not change.
- **Verification** (unit + differential): per retired binding, the golden record and the card's tagged tests are identical in Rust; differential fuzz over decks containing the affected cards shows zero digest differences; the coverage report shows the binding count falling.
- **Status:** open

### W-64 — Card-specific custom hooks, several duplicated; custom steps cannot request choices
- **Category:** effects · **Resolution type:** Rust fix (consolidated hooks, behaviour-identical; form depends on the IR decision)
- **Sources:** `src/gcg_sim/effects/dsl.py:6-7`, `src/gcg_sim/engine/interp.py:1-6`, `src/gcg_sim/effects/builtins.py:13-65`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:24`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:52-61`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:115-121`, `src/gcg_sim/effects/bindings/wp_gd05_a.py:97`, `src/gcg_sim/effects/bindings/wp_gd05_a.py:122`, `src/gcg_sim/effects/bindings/wp_gd01_b.py:117`, `src/gcg_sim/effects/bindings/wp_st12_14.py:83`, `src/gcg_sim/effects/bindings/wp_st12_14.py:96`, `src/gcg_sim/effects/bindings/wp_st05_08.py:46`, `src/gcg_sim/effects/bindings/wp_eb01_a.py:61`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:240`, `src/gcg_sim/effects/bindings/wp_gd02_b.py:54`, `src/gcg_sim/effects/bindings/wp_gd03_a.py:92`, `CLAUDE.md:44-47`, `docs/port/inventory/04-effects-ir.md:950-952`
- **Quotes:** “modules are deterministic and never request choices.”; “Prefer a general compiler template or a DSL/engine feature over a card-specific hook.”; “Will the Rust engine load a Python-exported IR (recommended) or must it also compile card text”
- **What is weak:** 29 hooks (10 `@custom_step`, 8 `@custom_filter`, 6 `@custom_cond`, 5 `@custom_value`) plus 5 builtins; duplicated purposes: `played_with_ex` (gd04_b, gd05_b, with different semantics: most recent play vs this turn), destroyed-by-effect / with-damage (gd05_a, gd01_b, st12_14), look at own top card (eb01_a, eb01_b), pay cost to deploy (gd02_b, gd03_a), lowest/highest enemy Lv. (gd05_a, st12_14, st05_08). Custom steps cannot request choices.
- **Resolution plan:** depends on the IR decision recorded in W-63. Custom hooks are Python functions, so in both cases Rust implements each hook natively, keyed by its hook id. (a) Exported IR: the exported IR keeps the Python hook ids. Rust maps each duplicated family to one implementation, with each card's exact semantics as a parameter where the copies differ, so no Python change is needed. Turning a family into a real DSL node is a Python binding change handed to the lead. (b) Ported compiler: each duplicated family becomes one typed DSL node in the Rust effect layer, keeping each card's exact semantics as a node parameter. In both cases, hooks without duplicates stay as named Rust functions in one module, and a change is accepted only when golden-identical (as W-63).
- **Verification** (unit + differential): every card using a consolidated hook passes its tagged tests and golden check; a test asserts no two hooks share a name-normalised purpose; differential fuzz clean.
- **Status:** open

### W-65 — The keyword and timing vocabulary is duplicated across many files
- **Category:** effects · **Resolution type:** Rust fix (single source)
- **Sources:** `.claude/skills/gcg-refresh-data/references/engine-extension.md:9-31`
- **Quotes:** “The keyword vocabulary appears in several places. Update all of them.”
- **What is weak:** the vocabulary lives in `dsl.Kw`, the compiler's `abilities._KW`, `nouns.KW_RE`, `grammar._KWS`, the conflicts `KEYWORD_VOCABULARY`/`TIMING_VOCABULARY`, the registry's keyword programs and the AI evaluation.
- **Resolution plan:** Rust defines keywords and timings once (an enum with its text forms and program), and every other table is derived from it with exhaustive matches, so adding a keyword forces every consumer to handle it.
- **Verification** (unit): a test asserts the derived tables equal the Python vocabularies on the packaged data; adding a variant without handling fails to compile.
- **Status:** open

### W-66 — The compiler miscompiled some cards silently; bindings fixed the cases found
- **Category:** effects · **Resolution type:** Rust fix (differential audit)
- **Sources:** `src/gcg_sim/effects/bindings/wp_gd04_a.py:229`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:489`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:508`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:539`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:553`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:669`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:684`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:334-336`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:395`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:450-452`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:169`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:232`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:290`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:316`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:210`, `src/gcg_sim/effects/bindings/wp_gd04_b.py:223`
- **Quotes:** “compiled AP reduction recounted the trash continuously instead of fixing it”
- **What is weak:** binding notes record silent miscompiles: resting one Unit when the other target was missing (Q265); an optional choice without a minimum; damaging self with no enemy chosen; a Pilot referenced through the destroyed Unit; only the first mode compiled and the second run unconditionally; friendly attacks retargeted; an event payload read in a Command frame that lacks it; "belonging to each enemy player" compiled as either side; a dropped "… and 1 enemy Unit"; an EX-Resource variant granting nothing. Other compiled cards may share the same patterns.
- **Resolution plan:** a pattern audit: for each miscompile family, a static scan of the IR of every compiled (non-binding) card for the same shape (e.g. a two-target step without an all-targets guard, an optional choice without minimum, a mode list whose second mode is unconditional); each hit gets a tagged behaviour test against the card's text and rulings. Fixes go through the Python data/binding workflow if a real miscompile is found, since card behaviour must not change in the port alone.
- **Verification** (unit): the scan runs in the Rust test suite and lists zero unreviewed hits; each reviewed hit has a tagged test passing in both implementations.
- **Status:** open

### W-67 — No templates for "all players each …" and related multi-player-step wording
- **Category:** effects · **Resolution type:** general templates, golden-identical (Rust or Python side, after the IR decision)
- **Sources:** `src/gcg_sim/effects/bindings/wp_eb01_b.py:89`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:103`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:135`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:161`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:221`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:277`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:144-147`, `docs/port/inventory/04-effects-ir.md:950-952`
- **Quotes:** “'All players each choose 1 of their Resources' has no template (Q321-Q323)”; “Will the Rust engine load a Python-exported IR (recommended) or must it also compile card text”
- **What is weak:** also missing: "place the top card of your deck into your trash", "They may draw 1", "reveal 1 … return it to the top", "all enemy players each choose", "All players each look at the top card", and "After activating this card's 【Main】, you may pair this card from your trash" (modelled inline).
- **Resolution plan:** depends on the IR decision recorded in W-63. The content of the change is one general "each player in turn order" step (with the simultaneous rules-management deferral of Q319/Q331) plus the listed single-player templates. (a) Exported IR: the step and the templates go into the Python DSL and compiler through the binding workflow (handed to the lead), and Rust adds the one IR node the step needs. (b) Ported compiler: both go into the Rust effect layer. Either way, the matching bindings are retired only when golden-identical (as W-63).
- **Verification** (unit + differential): golden and tagged tests for every affected card identical; differential fuzz clean.
- **Status:** open

### W-68 — Stale "compiled to printed HP" binding notes: Stat.HP already means current HP
- **Category:** effects · **Resolution type:** Rust fix (dedupe, behaviour-identical)
- **Sources:** `src/gcg_sim/engine/view.py:673-685`, `src/gcg_sim/effects/bindings/wp_gd01_a.py:78-85`, `src/gcg_sim/effects/bindings/wp_st01_04.py:19`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:34-36`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:164`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:186`
- **Quotes:** “HP referred to by card text is the current HP (printed/modified HP minus damage)”; “the compiled HP filter read printed HP instead of current HP (FAQ Q96)”
- **What is weak:** `view.stat()` evaluates `Stat.HP` as `max(0, hp - damage)`, the same as `REMAINING_HP` (verified in the code this session), so bindings whose only purpose is the HP fix (GD01-004, the ST01–04 `_CURRENT_HP_NOTE` cards, GD05-072, GD05-083) may be redundant, and the DSL has two stats with identical semantics.
- **Resolution plan:** Rust maps both stats to one implementation; for each HP-fix binding, compare its IR behaviour with the compiled script and retire it only when golden-identical (as W-63); the stale notes are listed for the Python side.
- **Verification** (unit): for each listed card, compiled and bound scripts give identical results on a set of HP/damage states; the card's tagged tests pass.
- **Status:** open

### W-69 — The registry build swallows binding errors; the card silently becomes unimplemented
- **Category:** effects · **Resolution type:** Rust fix (fail fast)
- **Sources:** `src/gcg_sim/effects/registry.py:325-339`, `tests/effects/test_card_coverage.py:22-29`
- **Quotes:** “except (ValueError, AssertionError, TypeError, KeyError, IndexError, AttributeError) as exc:”
- **What is weak:** a bug in a binding becomes a "binding error" entry that surfaces only when a game needs the card (`UnimplementedCardError`) or when the coverage test runs.
- **Resolution plan:** the Rust registry build returns an error listing every card that failed to load (bindings are code, so a failure is a build bug); "unimplemented" stays a separate, data-driven state for cards with no script.
- **Verification** (unit): a test registry with a deliberately broken binding fails to build with that card named; the packaged registry builds with zero errors.
- **Status:** open

### W-70 — Historical compiler and DSL bugs fixed in Python: regression risk for the port
- **Category:** effects · **Resolution type:** port-preserve + regression tests
- **Sources:** `102e72b`, `7c47b86`, `eccb538`, `eccb538:tests/rules/test_effects.py`, `b369dc8`, `c8cc8e9`, `f89c0cc`, `21489cb`, `9408681`, `0ee591d`, `aa0dd30`, `5c46d63`, `db94768`
- **Quotes:** “ENGINE: 'Then' part compiled outside the leading 'If' (GD03-072 discards without the condition)”; “Development cards not compiling (13-1-8-1/2)”; “start-phase freezes (EB01-001, ST14-001) use the stays-rested rule instead of phase conditions”; “fix fragile bindings, token-definition template, simultaneous battle damage resolution”
- **What is weak:** Then/If scoping (GD03-072, GD02-021), "Choose 2" playable with one target, "instead" losing "during this turn" (ST07-009) or replacing the damage step (GD03-109), multi-target pre-checks; cards that did not compile (ST03-011, GD03-085, GD05-113, 【Deploy･Development N】); missing DSL support for deployed-rested statics (Q354) and deploying a Pilot as a Unit (Q389/Q390); start-phase freezes (EB01-001, ST14-001) modelled as the stays-rested rule (Q208/Q288); fragile per-card bindings repaired at integration. The compiler is fragile on split or unusual lead sentences.
- **Resolution plan:** as W-57: every tagged test ported with its tag; the regression table covers these commits; golden records compared between implementations.
- **Verification** (unit + meta-test): ported tests pass; the meta-test covers each listed fix; Rust golden output equals Python's for every card.
- **Status:** open

## Validator

### W-71 — The ST02/ST05 unmodified starter-deck exceptions are not applied
- **Category:** validator · **Resolution type:** Rust fix (deferred until data exists) + reporting
- **Sources:** `README.md:172-174`, `docs/DECK_FORMAT.md:69-72`, `docs/CONFLICTS.md:975-982`, `docs/research/OFFICIAL_SOURCES.md:88-94`, `docs/research/OFFICIAL_SOURCES.md:247-250`, `src/gcg_sim/deck/validate.py:17`, `src/gcg_sim/deck/validate.py:29-30`, `src/gcg_sim/data/official/banlist.json:15`, `src/gcg_sim/data/official/banlist.json:140`, `src/gcg_sim/data/official/banlist.json:327`, `data/sources.d/official.json`
- **Quotes:** “The validator cannot tell that a deck is an unmodified starter deck, so it reports them as violations and says so.”; “is not applied: the simulator cannot verify an unmodified starter deck”
- **What is weak:** the unmodified ST02 deck (3x ST02-016 Corsica Base) and ST05 deck (ST05-004 + ST05-009 vanilla pair) are tournament-legal officially but fail as RESTRICTED / ATTRIBUTE_PAIR; neither the official pages nor gcg-api contain the starter decklists (the product entries in `data/sources.d/official.json` say "no decklist"); support is planned only if an official decklist is added with provenance.
- **Resolution plan:** the Rust validator gets the exception logic behind a data hook: if the packaged data ever contains an official starter decklist with provenance, a deck whose multiset equals it is exempt from exactly the listed restriction; until then the validator's message keeps saying why it is reported. Adding the decklist is a data-refresh task outside the port.
- **Verification** (unit): with a synthetic decklist fixture, an exact ST02 list passes and a one-card variation fails; with the packaged data (no decklist), both are reported with the explanatory note as in Python.
- **Status:** open

### W-72 — One current B&R list: no history and no region
- **Category:** validator · **Resolution type:** Rust fix (schema-ready, data unchanged)
- **Sources:** `docs/CONFLICTS.md:957-964`, `docs/research/OFFICIAL_SOURCES.md:96-102`, `docs/research/OFFICIAL_SOURCES.md:250-252`, `.claude/skills/gcg-refresh-data/references/sources.md:70-73`, `src/gcg_sim/data/official/banlist.json:20`, `src/gcg_sim/deck/official.py:112-127`
- **Quotes:** “The simulator validates against one list, not by date.”
- **What is weak:** only the list with effective date 2026-09-25 (the page date) exists; the US and Asia July lists took effect a day apart (2026-07-24 vs 07-25); a deck cannot be validated as of an earlier date or for a region, and each B&R update means hand-editing `banlist.json`.
- **Resolution plan:** the Rust validator's B&R model is a list of dated, region-tagged entries, of which the packaged data currently provides one; `--as-of DATE` and `--region` select an entry and fail clearly when no entry covers the request. Adding historical lists is a data-refresh task; the port does not change the packaged list.
- **Verification** (unit): with the packaged data, default validation equals Python's; with a two-entry fixture, `--as-of` before the second date uses the first list; a date before every entry is an error.
- **Status:** open

### W-73 — Vanilla banned pair: the predicate and the enumerated list can diverge, and divergence only adds a note
- **Category:** validator · **Resolution type:** data tripwire
- **Sources:** `docs/CONFLICTS.md:984-991`, `docs/research/OFFICIAL_SOURCES.md:243-244`, `.claude/skills/gcg-refresh-data/SKILL.md:171-186`, `src/gcg_sim/data/official/banlist.json:5`, `src/gcg_sim/deck/validate.py:202-218`
- **Quotes:** “so the list can lag a new set.”
- **What is weak:** membership is predicate OR the enumerated 22-card list; when they disagree the violation message adds a "Note:"; failing loudly on divergence depends on the refresh tooling, not on the validator.
- **Resolution plan:** a Rust data-gate test evaluates the predicate over the packaged pool and requires it to equal the enumerated list; the validator's runtime behaviour (predicate OR list, note on divergence) stays as in Python.
- **Verification** (unit + negative control): the test passes on the packaged data (22 matches); a synthetic vanilla Lv2/cost1/2AP/2HP Unit in a temporary data copy makes it fail.
- **Status:** open

### W-74 — Validator predicates that hold only for today's pool: "without effects", primary-name matching, one colour per card
- **Category:** validator · **Resolution type:** data tripwire
- **Sources:** `docs/CONFLICTS.md:993-1000`, `docs/research/OFFICIAL_SOURCES.md:245-246`, `docs/research/OFFICIAL_SOURCES.md:124-125`, `docs/research/DATA_INGESTION.md:220-221`, `src/gcg_sim/data/official/banlist.json:10`, `src/gcg_sim/deck/official.py:130-132`, `src/gcg_sim/deck/official.py:153-154`, `src/gcg_sim/deck/validate.py:133-151`
- **Quotes:** “'without effects' is not defined.”; “return str(value) in card.name”; “every UNIT, PILOT, COMMAND and BASE card number has exactly one color.”
- **What is weak:** "without effects" is read as effect text non-empty and not "-" (keywords count, a link alone does not); no current card is affected. The `name_contains` predicate tests only the primary name, not alias names (rule 2-2-4: GD02-098, ST12-011, ST12-012 and the 68 【Pilot】 Commands), so a future name-based B&R rule would miss alias holders (verified in the code this session). `check_colors` assumes one colour per card and skips colourless main-deck cards.
- **Resolution plan:** Rust validator keeps the same behaviour; tripwires fail the data gate when a card would be classified differently by the alternative reading (a Lv2/cost1/2AP/2HP Unit with only keywords or only a link), when a B&R entry uses `name_contains` with a value matching any alias of a card that its primary name does not, or when any card has more than one colour.
- **Verification** (unit + negative control): tripwires pass on the packaged pool; synthetic cards in a temporary data copy trigger each one.
- **Status:** open

### W-75 — The B&R list is enforced by default although the Comprehensive Rules never mention it
- **Category:** validator · **Resolution type:** port-preserve + reporting
- **Sources:** `docs/CONFLICTS.md:948-955`, `docs/DECK_FORMAT.md:87`, `src/gcg_sim/data/official/banlist.json:25`, `src/gcg_sim/deck/validate.py:268-289`, `src/gcg_sim/reports/summary.py:200-209`
- **Quotes:** “Enforce the B&R list by default, because benchmarks model tournament play”
- **What is weak:** `validate_deck(deck, banlist=False)` checks only the Comprehensive Rules (6-1-1-3 allows 4 copies of anything); the CLI always enforces B&R; only "standard constructed" is supported.
- **Resolution plan:** keep the default (tournament play). The Rust CLI exposes the existing library switch as `--no-banlist`. Default reports already state the list's date (`versions.banlist_effective_date` in `results.json`, and the Versions section of `summary.md`, `summary.py:200-209`), and that stays byte-identical. The format applied (Constructed, the only one supported) and whether a list was applied go into `analysis.json` (written only with `--analysis`, W-90). A `--no-banlist` run is opt-in and has no Python CLI equivalent, so only that run's report header carries a warning that no B&R list was applied.
- **Verification** (unit + e2e): default CLI validation and default reports equal Python's. `--no-banlist` accepts a 4-copy deck of a banned card, and that run's report header carries the warning. `analysis.json` names the format and the list date.
- **Status:** open

### W-76 — Tournament-only deck rules, limited formats, tournament procedures and Edition Beta legality are not modelled
- **Category:** validator · **Resolution type:** reporting (explicit scope, additive outputs)
- **Sources:** `docs/research/OFFICIAL_SOURCES.md:126-128`, `docs/research/OFFICIAL_SOURCES.md:191-198`, `docs/research/OFFICIAL_SOURCES.md:199-210`, `docs/research/OFFICIAL_SOURCES.md:233-236`, `docs/DECK_FORMAT.md:72-73`, `docs/CONFLICTS.md:31-32`, `src/gcg_sim/data/official/deck_construction.json:158-161`, `src/gcg_sim/data/official/edition_language.json:281`, `docs/research/OFFICIAL_SOURCES.md:48-51`, `data/official_raw/en_pdf_floor_rule_en.txt:571-581`, `src/gcg_sim/data/official/bo3_match_rules.json:16-21`, `docs/CONFLICTS.md:948-955`, `src/gcg_sim/reports/summary.py:67-226`, `src/gcg_sim/reports/results.py:28-38`
- **Quotes:** “the 7-day ban on newly released card numbers at Premier events;”; “No official statement on tournament legality was found”; “7-day new-card rule for Premier events (TRM 2.4.1; Sanctioned Floor Rules Section 5)”; “will construct a legal deck from that product only.”; “"intentional_draws_concessions_penalties": "Not modelled.",”
- **What is weak:** the following are not modelled: the 7-day new-card ban at Premier events (and the Asia Newtype Challenge rule), sleeves, card condition, deck lists, language and region rules, and judge deck checks (replaced by pre-match validation). Edition Beta printings are validated by card number only (RP-001 and EXBP-001 exist only in Edition Beta), and their tournament legality is undecided officially. Only the Constructed format is supported (W-75). The floor rules also define the limited formats Sealed ("Players will receive specific event product … and will construct a legal deck from that product only") and Draft. Sealed Battle is a separate RULES-tagged notice, listed apart from Team Battle and Battle Royale. These are two-player deck-construction formats, not multiplayer (W-131), and the explicit design decisions do not cover them. Intentional draws, concessions and penalties are tournament procedures, recorded as "Not modelled." in `bo3_match_rules.json`. They are not part of the clock (W-133).
- **Resolution plan:** the Rust validator adds an optional `--event-date DATE` check for the 7-day rule, using the release dates already in the packaged data (no data change; skipped with a note when a date is missing). An opt-in `gcg-sim validate --scope` lists the tournament rules the validator does not check and flags Edition Beta-only printings. It states the deck-construction format applied (Constructed), and lists Sealed and Draft as unsupported formats. Without the flag, `validate` prints what Python prints. Supporting Sealed would need its rules page (`news/sealed-battle.html`, linked from the Rules page but not stored) to be added through `/gcg-refresh-data`; that is a data task outside the port, handed to the lead. The byte-compared benchmark outputs stay unchanged: `summary.md` has no scope section (its sections run from Result to Notes, `summary.py:67-226`), and the `results.json` notes are fixed text (`results.py:28-38`). So the unmodelled tournament procedures (intentional draws, concessions, penalties) are listed in a `scope` block of `analysis.json` (written only with `--analysis`, W-90) and in the Rust report documentation.
- **Verification** (unit + e2e): a deck containing a card released 3 days before `--event-date` fails the optional check and passes without it. `validate --scope` lists the unchecked rules, flags RP-001, and names Constructed as the applied format with Sealed and Draft as unsupported, and `validate` without the flag prints what Python prints. `analysis.json`'s scope block lists the unmodelled tournament procedures, and the benchmark outputs of the same run without `--analysis` are byte-identical to Python's.
- **Status:** open

### W-77 — Deck file format limits
- **Category:** validator · **Resolution type:** Rust fix (diagnostics)
- **Sources:** `docs/DECK_FORMAT.md:22-26`
- **Quotes:** “There are no end-of-line comments, because card names may contain `#`.”
- **What is weak:** no end-of-line comments; listing an EX Resource (a token) fails CARD_TYPE and still counts toward RESOURCE_SIZE.
- **Resolution plan:** the Rust parser keeps the format and the violations (equivalence) and adds a hint to the CARD_TYPE message when the card is an EX token and to lines that look like "name  # comment" whose name does not resolve.
- **Verification** (unit): violation codes and counts equal Python's on the deck-format test corpus; the hints appear in the two cases.
- **Status:** open

### W-78 — Legality depends on simulator support (UNIMPLEMENTED)
- **Category:** validator · **Resolution type:** port-preserve + reporting
- **Sources:** `docs/DECK_FORMAT.md:67`, `docs/DECK_FORMAT.md:88`, `src/gcg_sim/deck/validate.py:268`
- **Quotes:** “Every card has an implemented effect in this simulator version.”
- **What is weak:** a new card missing from the registry makes a legal deck fail; `validate_deck(implemented=False)` skips the check.
- **Resolution plan:** keep the check (a benchmark cannot play an unimplemented card); the Rust report separates "illegal by the rules" from "not playable by this simulator version", and the CLI exposes the existing switch for validation-only use.
- **Verification** (unit): a deck with a synthetic unimplemented card reports UNIMPLEMENTED under the simulator-support heading and passes rules validation with the switch.
- **Status:** open

## Data

### W-79 — Official errata missing from gcg-api are applied by hand (ST12-001)
- **Category:** data · **Resolution type:** data tripwire (data unchanged)
- **Sources:** `docs/CONFLICTS.md:910-920`, `docs/research/OFFICIAL_SOURCES.md:212-214`, `docs/research/OFFICIAL_SOURCES.md:262-264`, `docs/research/DATA_INGESTION.md:206-210`, `.claude/skills/gcg-refresh-data/references/conflict-policies.md:80-82`, `src/gcg_sim/sources/conflicts.py:1603`, `data/sources.d/official.json`, `data/conflicts.json`, `8182f98`
- **Quotes:** “The pinned gcg-api snapshot (errata.json and cards.ndjson) still has the printed wording.”; “official errata (2026-09-04) adds 【Once per Turn】; missing from gcg-api”; “Apply official ST12-001 errata (news 02_193) through a curated conflict and field override”
- **What is weak:** the 2026-09-04 Epyon erratum is in neither `errata.json` nor `cards.ndjson`, and Bandai does not update card pages for errata; it is applied through an `overrides.json` field override (major severity), so every future erratum needs manual curation.
- **Resolution plan:** Rust reads the override as packaged. A data-gate test cross-checks every erratum recorded in the official-source data against the effective card text and fails if an erratum is neither in the gcg-api text nor covered by an override.
- **Verification** (unit + negative control): ST12-001's effective text has 【Once per Turn】; removing the override in a temporary copy fails the test.
- **Status:** open

### W-80 — Card text format defects corrected by overrides (GD02-053, GD03-015, GD03-035)
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `docs/CONFLICTS.md:1050-1086`, `docs/research/DATA_INGESTION.md:263-264`, `docs/research/EFFECT_TEMPLATES.md:95-97`, `scripts/cluster_effects.py:76-77`, `data/conflicts.json`
- **Quotes:** “so keyword_effects omits them and a template compiler would miss the keyword.”
- **What is weak:** GD02-053 prints [Suppression] in square brackets (major), and GD03-015 and GD03-035 use an ASCII ':' as the cost separator; a compiler must normalise them (rule N06) or it drops the keyword.
- **Resolution plan:** a data-gate scan of all card text for bracketed keyword names and ASCII cost separators; every hit must be covered by a normalisation rule or override.
- **Verification** (unit): the scan finds exactly the three known cards, each covered; GD02-053 has <Suppression> in Rust.
- **Status:** open

### W-81 — gcg-api structure fields (keyword_effects, timing_markers, link_refs) are lossy
- **Category:** data · **Resolution type:** port-preserve + test
- **Sources:** `docs/CONFLICTS.md:1005-1048`, `docs/research/DATA_INGESTION.md:252-254`, `docs/research/DATA_INGESTION.md:258-272`, `docs/research/EFFECT_TEMPLATES.md:98-104`, `src/gcg_sim/sources/conflicts.py:1004-1015`, `data/conflicts.json`
- **Quotes:** “The gcg-api extractor is lossy by design”; “【Pilot】 sections are not recorded in timing_markers”; “It is a mention list and should not be read as the card's own keywords.”
- **What is weak:** `keyword_effects` lists keywords a card only mentions or grants (GD04-067 lists all seven); `timing_markers` drops 【Pilot】, Development N and pilot qualifiers and splits Activate･Main; `link_refs` loses the name/trait distinction and adds a spurious "Machu" on 14 printings. Abilities must come from the text (5 conflicts use `engine_derives_markers`).
- **Resolution plan:** the Rust data model never reads these fields for behaviour; a test asserts no engine or AI code path depends on them.
- **Verification** (unit): a test build with the three fields blanked in a temporary data copy produces identical registry content ids and golden records.
- **Status:** open

### W-82 — A null printed AP is read as 0 (inference); EX Base AP is null in the source
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `docs/research/DATA_INGESTION.md:226-238`, `docs/CONFLICTS.md:1091-1120`, `data/conflicts.json`, `6078099`
- **Quotes:** “a zero AP arrives the same way (inferred, not stated upstream)”; “EXBP-035 added to data-null:EX-BASE:ap”
- **What is weak:** gcg-api maps a printed "-" and a zero AP both to null; 109 Base, 33 EX Base and 13 Unit printings (10 Unit numbers, plus tokens T-012/T-014) have null AP, read as 0; EX Base AP is filled through a conflict resolution.
- **Resolution plan:** keep the reading; a tripwire lists every null-AP printing and fails when a new one appears that is not a Base, EX Base or a reviewed Unit/token, so each new case is reviewed.
- **Verification** (unit + negative control): the list on the packaged data equals the reviewed set; a synthetic null-AP Unit fails it.
- **Status:** open

### W-83 — Edition Beta stat records are defective (one-row shift in the source listing)
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `docs/research/DATA_INGESTION.md:185-196`, `src/gcg_sim/data/overrides.json:228-231`
- **Quotes:** “The Edition Beta stat values are not real Beta stats.”; “That points to a one-row shift in the source listing”
- **What is weak:** T-002_p1, T-003_p1, T-006_p1 and R-001_p4 (a Resource with AP3/HP1) copy the previous record's stats, T-001_p1 has null AP/HP; curated overrides pick non-Beta canonical printings (e.g. R-001_p7). A new Beta-like printing needs the same handling.
- **Resolution plan:** a tripwire flags any printing whose stats equal the previous record's in source order while differing from its canonical printing, and any Resource with AP/HP; each hit must be covered by a canonical-printing override.
- **Verification** (unit): the tripwire finds exactly the known printings, all covered.
- **Status:** open

### W-84 — Link, name and trait anomalies in the card data
- **Category:** data · **Resolution type:** port-preserve + test
- **Sources:** `docs/research/DATA_INGESTION.md:131-133`, `docs/research/DATA_INGESTION.md:190`, `docs/research/DATA_INGESTION.md:247-256`
- **Quotes:** “`[Ericht Samaya]` (GD05-026) matches no pilot in the snapshot”
- **What is weak:** the GD05-026 link alternative names no existing Pilot; GD01-051_p1 has the malformed link "Trait [Enhanced Human]"; "(Dianna Counter)" is a trait no card carries yet; parsers must tolerate them.
- **Resolution plan:** the Rust parser tolerates them exactly as Python does; a data-gate report lists link targets and traits that match nothing, so new anomalies are reviewed.
- **Verification** (unit): the report on the packaged data equals the known anomalies; GD05-026 links as in Python.
- **Status:** open

### W-85 — Derived data files are curated by hand; curated conflict texts drifted from their resolutions
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `CLAUDE.md:34-43`, `.claude/skills/gcg-refresh-data/SKILL.md:193-197`, `32e9f84`
- **Quotes:** “Derived files are curated by hand”; “curated conflict texts resynced with resolutions”
- **What is weak:** `official/*.json` (verbatim quotes), `rules_na.json`, `overrides.json` and `curated_conflicts.json` are hand-curated; quotes are checked and hashes re-locked, but nothing structural keeps `curated_conflicts.json` in sync with `overrides.json`.
- **Resolution plan:** a Rust data-gate test joins `curated_conflicts.json` and `overrides.json` by conflict id and fails on any resolution text or policy mismatch, on orphaned entries on either side, and on hash drift against the lock.
- **Verification** (unit + negative control): passes on the packaged data; editing one resolution text in a temporary copy fails it.
- **Status:** open

### W-86 — Printing text divergences resolved to the base printing; a wrong rule citation was fixed in review
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `6078099`
- **Quotes:** “8 new printing divergences resolved to the base printing”; “divergent:RP-068: drop the claim attributed to rule 3-6, which only defines Resources.”
- **What is weak:** alternate printings whose text differs are treated as the base printing (7 reminder-only ST12/ST13 prints, RP-068_p1 "-"); a rationale once cited the wrong rule.
- **Resolution plan:** a tripwire diffs each alternate printing's normalised text (reminder text stripped) against its base printing and fails on any difference not covered by a `canonical_printing` conflict.
- **Verification** (unit): the divergences found equal the curated set.
- **Status:** open

### W-87 — The Comprehensive Rules file has wrong cross-references and typos (the source is never edited)
- **Category:** data · **Resolution type:** port-preserve + test
- **Sources:** `docs/research/RULES_INDEX.md:41-51`, `docs/research/RULES_INDEX.md:160-201`, `docs/CONFLICTS.md:1122`, `docs/CONFLICTS.md:1257-1269`
- **Quotes:** “Wrong cross-references (11 of the 34 references in the file)”; “Ten references still use the old numbers”
- **What is weak:** left over from the Ver. 1.1.0 renumbering (e.g. 2-7-1 "See 7. Attacking and Battles", 8-6-2 "See 6-5", 13-2-1-1 "see 9-1-7"), plus typos (11-3-1 "becomes zero less", 5-20-2, 13-2-11-1 "is set"); "exile" is undefined. Traceability cites the intended rule.
- **Resolution plan:** the Rust rules index loads the unmodified file and the same `rules_xref_intended_target` mapping; a test asserts every cross-reference resolves either directly or through the mapping.
- **Verification** (unit): 34 references, 11 mapped, 0 unresolved.
- **Status:** open

### W-88 — Provenance gaps: some official sources were read but not stored; cards.json is omitted from the package
- **Category:** data · **Resolution type:** reporting (provenance listing)
- **Sources:** `docs/research/OFFICIAL_SOURCES.md:42-46`, `docs/research/OFFICIAL_SOURCES.md:230-232`, `.claude/skills/gcg-refresh-data/references/sources.md:65-68`, `docs/SOURCES.md:20-22`, `docs/SOURCES.md:35`, `docs/ASSUMPTIONS.md:18-22`, `data/sources.d/official.json`, `data/sources.d/rules.json`
- **Quotes:** “That PDF is larger than 3 MB, so only its text extraction is kept.”; “are omitted from the package to keep the wheel small”; “PDF is 3,046,969 bytes (>= 3 MB), so it is not stored”
- **What is weak:** only the hash and text of the Asia floor-rules PDF are kept; news listing pages 2–14, four event pages, the decks page and the JP Newtype Challenge PDF were not retained; the rules markdown omits the PDF's revision history; `cards.json` and `cards/en/*.json` are omitted (equivalence with `cards.ndjson` is checked at ingestion); `data/` has no README.
- **Resolution plan:** the Rust data crate embeds the provenance fragments and exposes `gcg-sim data status --sources`, listing every source with its hash and whether its content is packaged, text-only or hash-only. Storing more raw sources is a data-refresh decision outside the port.
- **Verification** (e2e): the listing covers every entry of `data/sources.d/*.json` and flags the Asia PDF as text-only.
- **Status:** open

### W-89 — dataset_version's sha is the code commit that produced the data, not the data commit
- **Category:** data · **Resolution type:** port-preserve + docs
- **Sources:** `.claude/skills/gcg-refresh-data/references/sources.md:35-36`, `docs/research/DATA_INGESTION.md:23`, `docs/REPORTS.md:72`
- **Quotes:** “`dataset_version`'s sha is the code commit that produced the data, not the data commit that the lock pins.”
- **What is weak:** a provenance subtlety that `results.json` versions must keep and that readers easily misread.
- **Resolution plan:** Rust reports reproduce the field exactly (equivalence) and the Rust report documentation explains it next to the lock's data commit.
- **Verification** (unit): `versions.dataset_version` equals Python's for the packaged data.
- **Status:** open

## Reporting

### W-90 — Per-card and benchmark-card statistics are correlations inside AI-vs-AI games
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `docs/REPORTS.md:80`, `docs/REPORTS.md:100-102`, `.claude/skills/gcg-benchmark/SKILL.md:63-69`, `src/gcg_sim/reports/hypotheses.py:1-8`, `src/gcg_sim/reports/hypotheses.py:92`, `src/gcg_sim/reports/results.py:28-37`, `README.md:167-169`
- **Quotes:** “These are correlations inside AI-vs-AI games.”; “Correlation only: longer games also draw more cards.”; “Both seats are played by the same AI”
- **What is weak:** "win rate when drawn" and "benchmark card seen" signals are associations; cards drawn in longer games look better or worse for unrelated reasons (length bias); `loss_rate_lift` is association only; hypotheses are things to test, not conclusions.
- **Resolution plan:** keep the existing statistics byte-identical (equivalence) and add, in a separate `analysis.json` written only with `--analysis`, length-stratified per-card rates (strata by game length in turns, Mantel-Haenszel pooled difference) and, for the top hypotheses, a ready-to-run paired card-swap command (W-42) that turns the correlation into an intervention test.
- **Verification** (unit + e2e): unit test on a synthetic game set where a card's raw lift is entirely due to length: the stratified estimate is about zero; e2e: reports without `--analysis` are byte-identical to Python's.
- **Status:** open

### W-91 — Results are specific to the AI and preset, and not comparable across AI versions
- **Category:** reporting · **Resolution type:** Rust fix (guard in comparison tools)
- **Sources:** `docs/REPORTS.md:17`, `docs/REPORTS.md:63-65`, `src/gcg_sim/reports/results.py:31-32`, `.claude/skills/gcg-benchmark/SKILL.md:96-102`
- **Quotes:** “reports made before the October 2026 AI update (schema 1.0.0) are not comparable with new ones.”
- **What is weak:** both seats play the same AI, so results measure the decks under that AI, not under human play; comparisons need the same preset and `code_digest`; the new `standard` beats the old one 58% of the time; the Rust strengthened presets will again differ from the reference.
- **Resolution plan:** Rust `results.json` records the AI identity (implementation, preset, and for gcg-eval runs the reference or strengthened search), using the existing fields where they exist; every Rust comparison tool (`compare-runs`, gcg-eval) refuses to pair or pool runs whose AI identity or data version differ unless `--allow-mixed` is given, and prints why.
- **Verification** (unit): comparing a strengthened-`standard` run with a reference-`standard` run fails with a clear message; identical identities pass.
- **Status:** open

### W-92 — Game-level intervals and tests treat games within a BO3 match as independent
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `.claude/skills/gcg-benchmark/references/statistics.md:25-30`, `docs/REPORTS.md:66-68`, `src/gcg_sim/reports/aggregate.py:28-44`, `src/gcg_sim/reports/stats.py:12-25`, `src/gcg_sim/reports/stats.py:50-60`, `src/gcg_sim/reports/results.py:28-30`
- **Quotes:** “Games within a match are correlated (same decks, loser chooses first player)”; “`ci95` is a 95% Wilson score interval”
- **What is weak:** games 2–3 of a match share decks and the chooser depends on the previous game, yet game win rates, splits and per-card tests use plain Wilson intervals and pooled two-proportion tests over games, so their precision is overstated; the skill uses game counts for sample-size planning.
- **Resolution plan:** keep the existing game-level fields; `analysis.json` adds match-clustered intervals for game-level rates (cluster bootstrap over matches with a seeded RNG, plus the design effect), and the Rust port of the benchmark skill's sample-size guidance uses the design effect.
- **Verification** (unit): on simulated BO3 data with known within-match correlation, the clustered interval covers the true rate at about 95% over 1,000 replications while the plain interval under-covers.
- **Status:** open

### W-93 — Draws count as non-wins
- **Category:** reporting · **Resolution type:** port-preserve + reporting
- **Sources:** `docs/REPORTS.md:68`, `docs/AI.md:311`, `docs/AI.md:326`, `.claude/skills/gcg-benchmark/references/statistics.md:14`, `src/gcg_sim/reports/summary.py:36-40`, `15ae2eb`, `ef05730`
- **Quotes:** “Draws count as non-wins and are reported separately.”; “remove unused draw-as-half helpers”
- **What is weak:** the wins-only metric (adopted in ef05730; draw-as-half helpers removed in 15ae2eb) understates a deck that draws often, and the AI tuning validation still scores draws as 0.5 (W-40), so metrics are inconsistent across tools.
- **Resolution plan:** keep wins-only in every reference-equivalent output. All Rust tools (gcg-eval included) use the same wins-only definition from one shared function. The draw rate is printed next to the win rate only in gcg-eval output and in `analysis.json` (W-90). `summary.md` is byte-compared, so it keeps Python's Draws count column and prints no draw rate (`summary.py:36-40`).
- **Verification** (unit): a test asserts every rate-producing tool calls the shared function. For a run with draws, gcg-eval output and `analysis.json` print the draw count and the draw rate, and `summary.md` is byte-identical to Python's.
- **Status:** open

### W-94 — Hypothesis error control is per kind of comparison; thresholds are hand-set
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `docs/REPORTS.md:81`, `.claude/skills/gcg-benchmark/SKILL.md:68-69`, `src/gcg_sim/reports/hypotheses.py:5-7`, `src/gcg_sim/reports/hypotheses.py:54-65`, `src/gcg_sim/reports/hypotheses.py:199-205`, `ef05730`
- **Quotes:** “at most ``ALPHA`` per kind of comparison”; “Never act on a signal backed”; “noise-gated hypotheses (z-test + Holm)”
- **What is weak:** Holm runs separately for card deltas, benchmark threats, play/draw (a family of 1) and redraw (a family of 1, `MIN_REDRAWS = 10`), so the chance of at least one spurious hypothesis per run is up to about four times alpha; `MIN_GAMES 20`, `MIN_DELTA 0.05` and `MIN_SPLIT_DELTA 0.10` are hand-set; the skill advises never acting on fewer than about 50 observations.
- **Resolution plan:** keep the existing hypotheses byte-identical; `analysis.json` adds a run-level Holm adjustment across all families and the adjusted p-value of each hypothesis, so a reader can apply run-level control.
- **Verification** (unit): simulated null runs (no real effect) give a run-level false-positive rate at or below alpha with the run-level adjustment, and up to about 4 alpha without it.
- **Status:** open

### W-95 — "Never played" signals have no significance test or cap, and cost discards count as never played
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `src/gcg_sim/reports/hypotheses.py:7-8`, `src/gcg_sim/reports/hypotheses.py:103-122`, `src/gcg_sim/runner/instrument.py:20-22`, `src/gcg_sim/runner/instrument.py:98`, `docs/REPORTS.md:81`, `docs/REPORTS.md:95-97`
- **Quotes:** “"Never played" signals are descriptive”
- **What is weak:** the rule is 20+ drawn copies and a 50%+ never-played rate, with no test and no per-direction cap; "played" means only PLAY_UNIT, PLAY_BASE, PLAY_COMMAND or PAIR, so copies discarded for costs or used from hand by effects count as never played, yet the statement says the card "is often uncastable or low priority".
- **Resolution plan:** keep the existing signal; `analysis.json` splits "never played" into discarded for a cost, used by an effect from hand, still in hand at game end, and other, with a Wilson interval for each.
- **Verification** (unit): a synthetic game where a card is discarded for a cost reports it under "discarded for a cost", not "never played" in the analysis; the reference signal is unchanged.
- **Status:** open

### W-96 — Benchmark-card "seen" covers only played, Burst-revealed or public cards; lift is one-sided
- **Category:** reporting · **Resolution type:** Rust fix (additive analysis)
- **Sources:** `src/gcg_sim/runner/instrument.py:5-7`, `src/gcg_sim/runner/instrument.py:23-25`, `src/gcg_sim/reports/aggregate.py:184-185`, `src/gcg_sim/reports/aggregate.py:197-237`, `src/gcg_sim/reports/hypotheses.py:167-184`, `docs/REPORTS.md:80`
- **Quotes:** “Benchmark cards count as "seen" once they are played, deployed, paired, revealed as a Burst, or present in a public in-play zone.”
- **What is weak:** cards revealed from hand, discarded or seen in the trash are not counted; tokens are excluded; only positive lift (at least 0.10) becomes a hypothesis; benchmark cards get no play or draw statistics; per-card stats cover only the deck under test's main deck.
- **Resolution plan:** `analysis.json` adds a broader "observed" definition (any reveal, discard or trash entry), negative-lift reporting, and play/draw statistics for benchmark cards; the reference definitions stay unchanged.
- **Verification** (unit): a synthetic game where a benchmark card is only discarded is "observed" in the analysis and absent from the reference "seen" count.
- **Status:** open

### W-97 — The results schema file name stays v1 at version 1.1.0
- **Category:** reporting · **Resolution type:** port-preserve + docs
- **Sources:** `docs/ASSUMPTIONS.md:96`, `docs/REPORTS.md:59-60`
- **Quotes:** “file name stays `results.v1.schema.json`”
- **What is weak:** the file name tracks only the major version, which can confuse readers (1.1.0 added `fast` to `ai.preset`).
- **Resolution plan:** Rust ships the same schema file and name; a schema test asserts the `$id`/version inside the file and the `schema_version` written in results agree, and the Rust report docs state the naming rule (file name = major version).
- **Verification** (unit): Rust results validate against `results.v1.schema.json` and carry 1.1.0 (or the version the lead assigns for additive changes).
- **Status:** open

### W-98 — Distribution statistics are integer nearest-rank values and rounded rates
- **Category:** reporting · **Resolution type:** port-preserve + Rust fix (additive analysis)
- **Sources:** `docs/REPORTS.md:77`, `src/gcg_sim/reports/stats.py:9`, `src/gcg_sim/reports/stats.py:28-29`, `src/gcg_sim/reports/stats.py:50-73`, `src/gcg_sim/reports/stats.py:75-101`, `src/gcg_sim/reports/hypotheses.py:44-66`
- **Quotes:** “`median` is the ⌈n/2⌉-th smallest value (the lower middle value when n is even)”
- **What is weak:** median, p10 and p90 are nearest-rank integers (approximate for small n); the mean and every rate and p-value are rounded to 6 digits before the Holm comparison (`_compare` stores `rnd(two_proportion_p(...))`, and `_significant` passes those values to `holm`); byte-identical `results.json` requires exactly this.
- **Resolution plan:** Rust reproduces the nearest-rank rule and Python's rounding in every byte-compared output (the `gcg-core` round/format functions already verified against Python vectors). `analysis.json` (written only with `--analysis`, W-90) adds two things. For every distribution: the unrounded mean and the interpolated p10, median and p90 (linear interpolation between order statistics, Hyndman–Fan type 7). For every hypothesis comparison: the unrounded p-value, and the Holm decision recomputed from unrounded p-values, with a flag on every hypothesis whose decision differs from the reference's.
- **Verification** (unit): vectors exported from Python (odd and even n, ties, values at rounding boundaries) give identical Rust output for the reference fields. Additive fields: for the values [1, 2, 3, 4], the reference median is 2 and the interpolated median 2.5. In a Holm family of one at α = 0.05, an unrounded p-value of 0.0500004 rounds to 0.05, so the reference keeps the hypothesis, while the unrounded decision drops it, and the analysis flags it. The reference outputs are byte-identical with and without `--analysis`.
- **Status:** open

### W-99 — Decision logs make outputs large; reports and logs truncate
- **Category:** reporting · **Resolution type:** Rust fix (opt-in compression)
- **Sources:** `docs/REPORTS.md:19`, `src/gcg_sim/ai/base.py:15`, `src/gcg_sim/ai/base.py:84`, `src/gcg_sim/reports/hypotheses.py:23`, `src/gcg_sim/reports/hypotheses.py:128`, `src/gcg_sim/reports/hypotheses.py:205`, `src/gcg_sim/reports/results.py:27`, `src/gcg_sim/reports/results.py:90`, `src/gcg_sim/reports/summary.py:8`
- **Quotes:** “This makes them large.”; “LOG_ALTERNATIVES = 5”
- **What is weak:** `--decision-log` writes per-decision search statistics into `games.ndjson` and replays; the decision log keeps the top 5 alternatives, hypotheses keep 3 per direction, draw replays are capped at 1 and the summary shows 10 benchmark cards.
- **Resolution plan:** keep formats and limits byte-identical by default; add an opt-in `--compress` that writes `games.ndjson.zst` (decompresses to the identical bytes); full search detail goes to the gcg-eval trace (W-21) rather than raising the limits.
- **Verification** (unit + e2e): decompressed output is byte-identical to the uncompressed run; default output is unchanged.
- **Status:** open

### W-100 — The summary's reproduce command omits the agent factory and can name a deck that is not a path
- **Category:** reporting · **Resolution type:** Rust fix
- **Sources:** `src/gcg_sim/reports/writer.py:44-51`, `src/gcg_sim/cli.py:80`, `tests/bench/__pycache__/test_reproduce_line.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “decks = [d.source or d.name for d in (c.deck_under_test, c.benchmark_deck)]”; “summary.md's reproduce command carries PYTHONPATH and the agent factory of the run”
- **What is weak:** a run with `GCG_SIM_AGENT_FACTORY` (recorded under `ai.factory`) reproduces with the default AI; an in-memory deck without a source falls back to its name; the lost later branch fixed this.
- **Resolution plan:** the Rust reproduce line names the exact AI identity, and when a deck has no source file, it writes the deck to the report directory and points the command at it. Under W-108 the release CLI reads no agent-factory variable, so the AI identities a release command can reproduce are its presets (`--ai-preset`). A run whose agent was injected through the library API or the test-only build feature cannot be reproduced by a release command. For such a run, the reproduce line says so and names the injected agent recorded under `ai.factory`, instead of printing a command that would silently use the default AI. This changes `summary.md`, so the equivalence normaliser (`scripts/normalize_outputs.py`) must treat the reproduce line as an implementation field; the lead decides whether that is acceptable or the fix stays behind a flag.
- **Verification** (e2e): running the printed command from a fresh shell reproduces `results.json` byte for byte for a file deck, an in-memory deck and each non-default preset (`fast`, `strong`). For a run with an injected test agent, the reproduce line carries the "not reproducible with the release CLI" marker and names the agent.
- **Status:** open

### W-101 — conflict_resolutions lists only resolutions whose id names a deck card
- **Category:** reporting · **Resolution type:** Rust fix (additive field)
- **Sources:** `src/gcg_sim/reports/versions.py:93-99`, `src/gcg_sim/reports/versions.py:116-135`, `docs/REPORTS.md:82`
- **Quotes:** “numbers.update(cid.split(":")[1:])”
- **What is weak:** card numbers are inferred by splitting conflict ids plus curated lists; rule-level interpretations (e.g. `most_defensible_reading` on keywords or timing) that change how a deck's cards play are not listed, so the report understates which interpretations affected a run.
- **Resolution plan:** keep the existing field; `analysis.json` adds `interpretations_used`: every conflict or interpretation id whose program, rule or keyword was actually exercised in the run's games (recorded by the engine when an interpretation-tagged code path runs, per W-56).
- **Verification** (e2e): a run with a <Suppression> deck lists the Suppression-with-Base interpretation; the existing field is unchanged.
- **Status:** open

### W-102 — The report directory is not updated atomically as a set
- **Category:** reporting · **Resolution type:** Rust fix
- **Sources:** `src/gcg_sim/reports/writer.py:1-2`, `src/gcg_sim/reports/writer.py:79-91`, `src/gcg_sim/reports/writer.py:94-110`, `docs/REPORTS.md:26-28`
- **Quotes:** “so an interrupted write never leaves a corrupt file”
- **What is weak:** each file is written atomically, but stale replays are deleted and new replays, `games.ndjson`, `summary.md` and `timing.json` are written before `results.json`; an interruption after them leaves new files next to the previous run's `results.json`.
- **Resolution plan:** Rust writes the whole set into a sibling temporary directory and swaps it in with one rename (keeping the previous set until the swap), so readers see the old set or the new one, never a mix. File contents are unchanged.
- **Verification** (unit): a test that aborts the writer after each file leaves either the complete old set or the complete new set.
- **Status:** open

### W-103 — code_digest hashes every file of the installed package; the version can be "unknown"
- **Category:** reporting · **Resolution type:** Rust fix
- **Sources:** `src/gcg_sim/reports/versions.py:25-29`, `src/gcg_sim/reports/versions.py:32-52`
- **Quotes:** “return "unknown"”
- **What is weak:** in an editable or source checkout any stray non-`.pyc` file under `src/gcg_sim` changes the digest, so results look non-comparable; `package_version()` returns "unknown" when the package is not installed.
- **Resolution plan:** the Rust `code_digest` is computed at build time from the crate sources and embedded data listed in a manifest (no stray files), and the package version comes from the build; `scripts/normalize_outputs.py` already treats both fields as implementation-specific.
- **Verification** (unit): adding an unrelated file to the source tree does not change the digest; changing an engine source file does.
- **Status:** open

### W-104 — timing.json is wall-clock based and the only non-reproducible report
- **Category:** reporting · **Resolution type:** Rust fix (additive fields)
- **Sources:** `src/gcg_sim/reports/writer.py:60-76`, `docs/REPORTS.md:55`
- **Quotes:** “wall-clock measurements; the only non-reproducible report file”
- **What is weak:** `games_per_second` is games over wall seconds including worker start-up; no CPU-time measure is reported.
- **Resolution plan:** Rust `timing.json` keeps the wall-clock fields and adds per-worker CPU time, start-up time and a node/decision count (deterministic) so throughput can be compared across machines; timing stays out of every deterministic output.
- **Verification** (unit): fields present; decisions per game equal the count in `games.ndjson`; the equivalence comparison drops `timing.json` as now.
- **Status:** open

## Runner

### W-105 — Interrupted or failed runs discard all work: no checkpoint or resume
- **Category:** runner · **Resolution type:** Rust fix
- **Sources:** `docs/REPORTS.md:26-28`, `src/gcg_sim/runner/run.py:1-6`, `src/gcg_sim/runner/run.py:97-106`, `src/gcg_sim/runner/game.py:129-138`
- **Quotes:** “**Ctrl-C** stops the worker processes, writes no reports, and exits with 130.”; “so no partial results are returned”
- **What is weak:** a single game failure aborts the whole benchmark and Ctrl-C writes nothing, so a long run loses all finished matches.
- **Resolution plan:** the Rust runner appends each finished match record to a checkpoint file in the output directory (match index, seeds, record, config digest); `--resume` reloads it, verifies the config digest and plays only the missing matches. Because every match is a pure function of its seed, the final reports are byte-identical to an uninterrupted run. Failure semantics (exit code, no reports) stay the same unless `--resume` is used.
- **Verification** (e2e): kill a run after k matches, resume, and compare all reports byte for byte with an uninterrupted run; a resume with a different config is refused.
- **Status:** open

### W-106 — Worker termination: waitpid race, a private executor attribute, AI tools without interrupt handling
- **Category:** runner · **Resolution type:** Rust fix (by construction)
- **Sources:** `docs/PROGRESS.md:93-96`, `src/gcg_sim/runner/run.py:26`, `src/gcg_sim/runner/run.py:56-64`, `src/gcg_sim/ai/selfplay.py:113-118`, `src/gcg_sim/ai/tuning.py:165-169`, `src/gcg_sim/ai/tuning.py:292-296`, `d9d41af`
- **Quotes:** “the caller and the executor's manager thread both joined the terminated workers and raced for `waitpid`”; “executor._executor_manager_thread”; “joining them from the caller raced it for waitpid and left a reaped worker reported alive (test_interrupt_terminates_workers_and_returns_nothing failed 5/80 on main, 0/80 now)”; “with ProcessPoolExecutor(max_workers=workers, mp_context=get_context("spawn")) as pool:”
- **What is weak:** the interrupt test failed 5/80 before the fix; the fix relies on a private `ProcessPoolExecutor` attribute and a 10 s join timeout; `compare`, `throughput` and `tuning` use a bare pool without interrupt handling.
- **Resolution plan:** Rust runs games on threads (no worker processes, no `waitpid`) with a shared cancellation flag checked between decisions; results are collected in index order so worker count never changes output. gcg-eval and the tuning tool use the same runner.
- **Verification** (unit + e2e): an interrupt test run 200 times returns promptly with no reports every time; results identical for 1, 4 and 10 workers.
- **Status:** open

### W-107 — Spawn start-method constraints on the Python API
- **Category:** runner · **Resolution type:** Rust fix (not applicable by design)
- **Sources:** `docs/REPORTS.md:139`, `docs/REPORTS.md:154-156`
- **Quotes:** “required: workers are started with the "spawn" method”
- **What is weak:** the Python API needs a `__main__` guard and a picklable `agent_factory` when workers > 1.
- **Resolution plan:** the Rust runner uses threads, so neither constraint exists; agent factories are trait objects that are `Send + Sync`, checked at compile time. If Python bindings are offered, they release the GIL and run games in Rust threads.
- **Verification** (unit): a custom agent factory works with 10 workers in a test binary without any guard; a non-`Send` factory fails to compile (compile-fail test).
- **Status:** open

### W-108 — The GCG_SIM_AGENT_FACTORY test hook can replace the AI in a benchmark
- **Category:** runner · **Resolution type:** Rust fix
- **Sources:** `docs/REPORTS.md:149-156`
- **Quotes:** “Do not set it for real benchmarks.”
- **What is weak:** an environment variable swaps the search AI for any callable; it is recorded under `ai.factory` but easy to leave set.
- **Resolution plan:** the Rust CLI does not read an agent-factory environment variable; test agents are injected only through the library API or a test-only build feature, and `ai.factory` stays in results for equivalence.
- **Verification** (unit + e2e): the release CLI ignores the variable (results equal a run without it). The test feature's injected agent is recorded under `ai.factory`, and the summary's reproduce line marks that run as not reproducible with the release CLI (W-100).
- **Status:** open

### W-109 — Simultaneous defeat and the chooser after a drawn game rest on unconfirmed suggestions
- **Category:** runner · **Resolution type:** port-preserve + reporting
- **Sources:** `docs/ASSUMPTIONS.md:55`, `docs/REPORTS.md:38-42`, `docs/research/OFFICIAL_SOURCES.md:180-187`, `docs/research/OFFICIAL_SOURCES.md:260-261`, `docs/research/RULES_INDEX.md:209`, `src/gcg_sim/data/official/bo3_match_rules.json:2-12`
- **Quotes:** “In BO3 match scoring the turn player at the end loses (TRM 5.2 top-cut rule), so every match resolves.”; “No rule covers who chooses Player One after a drawn game.”; “Lead to confirm.”
- **What is weak:** a 1v1 double defeat is undefined by the rules (11-2-1 vs 1-2-1) and is a game draw in statistics; in BO3 scoring the turn player loses (TRM 5.2 is the top-cut rule but is applied to every BO3; in Swiss a drawn match scores 1 point). After a drawn game the game-1 random procedure picks the chooser. The packaged data file still carries "Lead to confirm." suggestion text.
- **Resolution plan:** Rust implements both exactly; every report states the number of double defeats and how they were scored, and an opt-in `--swiss-draws` scoring mode reports matches with a double-defeat decider as drawn (1 point) in `analysis.json` only. The data file's wording is a packaged source and stays unchanged; rewording it is a data-workflow task.
- **Verification** (unit + e2e): a scripted double defeat is a game draw and a BO3 loss for the turn player (equal to Python); the report counts it; `--swiss-draws` shows the alternative score only in the analysis.
- **Status:** open

## Tooling

### W-110 — Robustness fuzzing uses only random agents
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/SPEC.md:57`, `docs/PROGRESS.md:30-31`, `docs/status.json`, `src/gcg_sim/tools/fuzz.py:1-6`, `src/gcg_sim/tools/fuzz.py:46-73`, `src/gcg_sim/tools/fuzz.py:107-110`
- **Quotes:** “≥10,000 random-agent games, zero crashes, zero invariant violations”; “plays games with uniformly random legal actions”
- **What is weak:** invariants are checked after every action in random games with one- or two-colour generated decks; AI-driven lines are checked only through the derived-view cache guard in random and greedy games; turn-limit games count as failures.
- **Resolution plan:** Rust fuzz adds strategic agents (greedy and strengthened `fast` with a small node budget) and mixed-agent games over the same generated decks, with all invariants and the paranoid checks (W-49) on; seeds and decks are derived as in Python.
- **Verification** (e2e): 10,000 random and 1,000 strategic fuzz games with zero crashes and zero invariant violations; the random-agent fuzz results equal Python's on the shared seeds.
- **Status:** open

### W-111 — Coverage gates count tags, not behaviour
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `src/gcg_sim/tools/marks.py:1-5`, `tests/effects/test_card_coverage.py:33-40`
- **Quotes:** “Coverage gates and the rules traceability report are computed from the tags in the source,”
- **What is weak:** a card, rule, FAQ entry or ruling counts as covered when any test carries its tag; nothing checks what the test asserts.
- **Resolution plan:** Rust coverage combines tags with execution evidence: a tagged test counts only if, when run, it executes the tagged card's program (or the tagged rule's code path, via the interpretation ids of W-56), recorded by a test-only coverage hook.
- **Verification** (unit): a tagged test that never touches its card is reported as uncovered. The bar is 100% coverage with execution evidence (every card, testable rule, FAQ entry and ruling has a tagged Rust test that executes it, or a written N/A reason). Execution evidence can only lower the count below what tags alone give, so "the same coverage as the Python tags" is not a valid bar. The coverage report lists every Python test that is tagged but never executes its card or rule. Each such gap gets a ported test that does execute it, or is reported to the lead as a Python-side test gap.
- **Status:** open

### W-112 — The long slow suite is not deselected by default
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `pyproject.toml:53-62`, `scripts/verify_all.sh:23`, `tests/test_no_placeholders.py:10`, `tests/slow/test_robustness.py:15-24`, `tests/slow/test_ai_strength.py:43-56`, `tests/runner/test_replay.py:43-49`, `tests/ai/test_throughput.py:12`, `tests/ai/test_throughput.py:20`, `tests/ai/test_throughput.py:26`
- **Quotes:** “addopts = "-ra --strict-markers --import-mode=importlib"”; “@pytest.mark.slow”; “assert lower > 0.50”; “def test_every_example_pairing_replays(”
- **What is weak:** `addopts` has no `-m` filter (verify_all runs `-m ""`), so the default run includes the 10,000-game robustness test, the 400-game strength tests, the replay of every example pairing (6 permutations x 60 BO3 matches) and the throughput tests; there are no skip or xfail markers (which is good) and `src/` bans TODO/FIXME placeholders.
- **Resolution plan:** Rust tests are tiered: `cargo test` runs unit and fast tests; slow tests are `#[ignore]`d with a reason and run by `cargo test -- --ignored` in the full gate (never skipped there). No test is weakened or xfailed.
- **Verification** (e2e): the full gate script runs both tiers and fails if any ignored test fails; a meta-test asserts every ignored test belongs to the slow tier list.
- **Status:** open

### W-113 — Test helpers stop silently at step caps
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `src/gcg_sim/testkit.py:348-360`, `src/gcg_sim/testkit.py:363-380`, `src/gcg_sim/testkit.py:419-428`
- **Quotes:** “def pass_all(st: GameState, *, max_steps: int = 50) -> None:”; “for _ in range(200):”
- **What is weak:** `pass_all` stops after 50 steps, `to_next_turn` after 200 steps and `run_random` after 5,000 actions, all without an assertion. A test can therefore pass without reaching the state it intended. `to_next_turn` fails loudly only on an unexpected decision kind, not when it runs out of steps.
- **Resolution plan:** the Rust testkit helpers return an error (or panic in tests) when they hit their cap, with an explicit `allow_cap` variant for the rare test that wants it.
- **Verification** (unit): a helper forced past its cap fails the test; every ported test passes without `allow_cap` unless documented.
- **Status:** open

### W-114 — The documented-command checker: shell blocks only, a network-dependent skip, timeouts that leave children running
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/PROGRESS.md:52-53`, `docs/research/DATA_INGESTION.md:7`, `docs/status.json:15`, `docs/REPORTS.md:133-147`, `scripts/check_doc_commands.py:1-8`, `scripts/check_doc_commands.py:22-32`, `scripts/check_doc_commands.py:86-99`, `tests/bench/__pycache__/test_check_doc_commands.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “1 skipped with a reason: it needs a network clone”; “doc-check: skip needs a full git clone of gcg-api at f57b7c0 (network)”; “(?P<lang>bash|sh|shell)”; “runs each documentation block in its own process group and”
- **What is weak:** only bash/sh/shell fences run (the REPORTS Python API example never does); `.claude/skills/gcg-refresh-data` docs are not in the default files; the `ingest_profile.py` full-clone block is skipped; `subprocess.run(timeout=…)` kills only the bash child, so worker pools started by a block outlive it (the fix exists only in lost bytecode).
- **Resolution plan:** the Rust-side doc checker (used for the port's own docs) runs shell and Rust example blocks, puts each block in its own process group and kills the group on timeout or exit, includes the skill docs, and marks network blocks as "network" so a network-enabled CI job runs them instead of skipping them forever.
- **Verification** (unit): a doc block that starts a background `sleep 600` is killed with its group at the timeout; a Rust example block compiles and runs; the network block runs in the network job.
- **Status:** open

### W-115 — Research artifacts are pinned to an old snapshot and outside the gate; regeneration is manual
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/ASSUMPTIONS.md:43-46`, `.claude/skills/gcg-refresh-data/SKILL.md:287-291`, `.claude/skills/gcg-refresh-data/references/file-map.md:20-21`, `scripts/ingest_profile.py:31`, `6078099`
- **Quotes:** “`docs/research/data_profile.json` and `DATA_INGESTION.md` stay a profile of `f57b7c0`”; “Bump cluster_effects.py's GCG_API_COMMIT and regenerate effect_templates.json, work_packages.json and EFFECT_TEMPLATES.md”
- **What is weak:** `ingest_profile.py` (`PINNED_COMMIT`) and `cluster_effects.py` (`GCG_API_COMMIT`) pin commits by hand and are not part of the verification gate, so they drift silently (see W-128).
- **Resolution plan:** the port treats research artifacts as Python-side documents: Rust reads none of them. A Rust data-gate check compares the snapshot commit stated in each research artifact with the packaged lock and prints a "stale research artifact" warning in the gate summary (non-failing, since regenerating them is a Python-side task handed to the lead).
- **Verification** (unit): with the current files the check reports `DATA_INGESTION.md` and `data_profile.json` as profiles of f57b7c0 against the packaged 39c85b5.
- **Status:** open

### W-116 — Research scripts disagree with the engine on canonical printing and reminder-text stripping
- **Category:** tooling · **Resolution type:** port-preserve + test
- **Sources:** `scripts/cluster_effects.py:76-82`, `scripts/cluster_effects.py:108-112`, `scripts/cluster_effects.py:540-556`, `docs/research/EFFECT_TEMPLATES.md:82-84`, `docs/research/EFFECT_TEMPLATES.md:942`, `docs/research/DATA_INGESTION.md:46-48`, `docs/research/DATA_INGESTION.md:123-133`, `src/gcg_sim/cards/db.py:100-109`, `src/gcg_sim/data/overrides.json:63`, `src/gcg_sim/data/overrides.json:69`, `src/gcg_sim/data/overrides.json:228-231`
- **Quotes:** “R-001 -> R-001_p4”; “its content ends with '.' or has 5+ words”
- **What is weak:** `cluster_effects` takes the lowest `_pN` (Edition Beta, including the defective R-001_p4), `ingest_profile` the lowest non-Beta, the engine an override-pinned printing (R-001_p7) with a fallback that sorts `product_id` lexicographically (`_p10` < `_p2`); the scripts use two different reminder-stripping heuristics and the engine compiler a third. Harmless today only because these cards are vanilla.
- **Resolution plan:** Rust implements only the engine's rule (overrides, then the same fallback, reproduced exactly) and a test asserts that for every card number the fallback is either unused or agrees with the override, so the lexicographic quirk can never decide a non-vanilla card.
- **Verification** (unit): for every card number Rust and Python pick the same canonical printing; the fallback-only cards are all vanilla.
- **Status:** open

### W-117 — The adversarial review was a sample
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/SPEC.md:64`, `docs/PROGRESS.md:55-68`, `src/gcg_sim/tools/fuzz.py:33-43`
- **Quotes:** “stratified rules sample (54 rules), 23 N/A reasons, 64 cards”
- **What is weak:** 64 of 1,152 cards and 12 replays were audited; the other cards' behaviour rests on tests and goldens.
- **Resolution plan:** for the port, the differential check is exhaustive instead of sampled. Every card's golden record and tagged tests run in both implementations. Differential fuzz uses generated decks that jointly contain every card a deck can hold, which is 1,064 numbers: 956 coloured main-deck cards and 108 Resources. The other 88 numbers (29 UNIT TOKEN, 30 EX BASE, 29 EX RESOURCE) cannot be in a deck. They are covered by their golden records and tagged tests, and by fuzz games in which an effect creates them. The fuzz execution record (W-151) shows which programs actually ran, because a card being in a deck does not mean its program ran.
- **Verification** (e2e): the differential report has zero differences, split into four parts. Golden records and tagged tests: 1,152/1,152 numbers. Deck membership in differential fuzz: 1,064/1,064. Execution in differential fuzz: per card with a program, run or not, every card that never ran listed with the test that covers it instead. Effect-created token and EX numbers: created in fuzz, or covered by a golden record and a tagged test (88/88).
- **Status:** open

### W-118 — verify_all.sh gate limits
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `scripts/verify_all.sh:2-5`, `scripts/verify_all.sh:19-30`, `scripts/verify_wheel_offline.sh:10`, `scripts/verify_wheel_offline.sh:26`, `scripts/verify_wheel_offline.sh:34-38`, `.claude/skills/gcg-refresh-data/SKILL.md:291`
- **Quotes:** “the full gate on a clean clone of the committed HEAD (uncommitted”
- **What is weak:** it verifies only committed HEAD; it does not run `check_doc_commands.py`, the 500-game fuzz CLI, the traceability regeneration or the research-artifact checks; the wheel is picked by `ls … | tail -n 1` (lexicographic, wrong with several wheels); the offline check uses one Python version and one pairing with 2 matches for the worker-count identity.
- **Resolution plan:** the Rust full-gate script runs on a clean clone: fmt, clippy, all test tiers, the doc checker, the fuzz CLI, traceability regeneration with a diff check, the equivalence comparison and the offline binary check with several pairings and worker counts 1/4/10; artifacts are selected by exact version, never by sort order.
- **Verification** (e2e): the gate fails when any step fails (checked by injecting a failing step in a test copy) and passes on the clean branch.
- **Status:** open

### W-119 — blind_transcripts.py uses a private helper and path-dependent ids, untested
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `scripts/blind_transcripts.py:32`, `scripts/blind_transcripts.py:113-121`
- **Quotes:** “from gcg_sim.runner.replayfile import _decklist, describe_action”
- **What is weak:** the script imports the private `_decklist`, its game ids hash the actions plus the file path (they change with the directory layout), and no test covers it.
- **Resolution plan:** the Rust port (gcg-eval `transcripts`, used by W-17) uses public replay APIs, derives ids from the replay content only, and has a test.
- **Verification** (unit): the same replay in two directories gets the same id; transcripts contain no search statistics.
- **Status:** open

### W-120 — Project Claude permissions are broader than the "only the refresh skill uses the network" rule
- **Category:** tooling · **Resolution type:** configuration change (needs the user's approval)
- **Sources:** `.claude/settings.json:5`, `.claude/settings.json:13-15`, `CLAUDE.md:60-61`
- **Quotes:** “"Bash(curl -sS *)",”; “It is the only workflow that uses the network.”
- **What is weak:** `curl` to any host, `python3 *` and `uv *` are allowed project-wide while the project rules restrict network use to `/gcg-refresh-data`; `Bash(git -C * )` has a trailing space, so it may not match `git -C path cmd`.
- **Resolution plan:** proposed change, to be applied only with the user's approval (configuration is outside the port's files): replace the generic `curl` rule with the existing `WebFetch(domain:…)` entries or host-specific curl patterns, and remove the trailing space from the `git -C` rule.
- **Verification** (unit): a check that parses `.claude/settings.json` and fails on a host-unrestricted network command or a pattern with trailing whitespace.
- **Status:** open

### W-121 — Refresh tooling: only the offline core is tested; official fetches need a TLS workaround
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `5fc3f8b`, `docs/ASSUMPTIONS.md:47-48`, `6078099`
- **Quotes:** “refresh.py is the offline, tested core; skill scripts fetch (git sparse clone, official pages)”; “the macOS bundle that uv-managed Pythons read lacks GlobalSign Root R46”; “Refresh skill: verify official-site TLS with certifi's CA bundle”
- **What is weak:** the network fetch and verify scripts are outside the tested core; `fetch_official.py` depends on certifi's CA bundle because the macOS bundle lacks GlobalSign Root R46.
- **Resolution plan:** the refresh workflow stays in Python (data is not ported); the port adds a Rust data-loader test against recorded fixtures of every fetched file format, so a refresh that changes a format fails in Rust CI before it ships; fetch scripts get recorded-response tests on the Python side, handed to the lead.
- **Verification** (unit): the Rust loader accepts the recorded fixtures and rejects a fixture with a renamed field with a clear error.
- **Status:** open

### W-122 — Untracked bytecode of an unmerged later branch (bench/, tests/bench, tests/perf)
- **Category:** tooling · **Resolution type:** tooling (inventory)
- **Sources:** `bench/__pycache__`, `bench/legacy_ai/__pycache__`, `bench/perf/__pycache__`, `tests/bench/__pycache__`, `tests/perf/__pycache__`, `tests/determinism/__pycache__`, `bench/__pycache__/weakness.cpython-315.pyc`, `bench/__pycache__/ablate.cpython-315.pyc`, `src/gcg_sim/ai/__pycache__/inference.cpython-315.pyc`, `src/gcg_sim/ai/__pycache__/lethal.cpython-315.pyc`, `src/gcg_sim/engine/__pycache__/viewdeps.cpython-315.pyc`
- **Quotes:** “The weakness benchmark (plan section 6.7): legacy ``standard`` against new ``standard``, per”; “Ablations of the SO-ISMCTS flag families”
- **What is weak:** `bench/` holds only `.pyc` files (ablate, calibrate, differential, fidelity, h2h, weakness, legacy_ai, perf and others), plus `tests/perf`, `tests/bench` and `tests/determinism`; none appear in git history and all are gitignored (`__pycache__/`, verified this session); they document abandoned work (SO-ISMCTS rewrite, `gcg_sim.ai.lethal`, `gcg_sim.ai.inference`, `engine/viewdeps.py`) that does not exist on this branch.
- **Resolution plan:** treat the bytecode as read-only evidence only (strings already mined into W-17, W-21, W-48, W-54, W-100, W-114, W-125, W-126); the port never imports or runs it. Deleting the untracked directories is a destructive step left to the user.
- **Verification** (unit): the Rust build and gate scripts never reference `bench/` or `__pycache__` (a grep test).
- **Status:** open

### W-123 — compare.py usability limits
- **Category:** tooling · **Resolution type:** eval tooling
- **Sources:** `src/gcg_sim/ai/compare.py:14-16`, `src/gcg_sim/ai/compare.py:58`, `src/gcg_sim/ai/compare.py:70-76`, `src/gcg_sim/ai/compare.py:89-93`, `src/gcg_sim/ai/compare.py:166-171`
- **Quotes:** “Decks: ``tests`` (``tests/ai/decks.py``, run from the repository root)”
- **What is weak:** the default deck set `tests` is unavailable from an installed wheel; decks are labelled `deck0`/`deck1` in first-seen order; override converters cover only int/float/str/bool fields; baselines exist only for `standard` and `strong` (no baseline `fast`).
- **Resolution plan:** gcg-eval embeds its deck sets (test decks, tuning decks, panel, examples) in the binary, labels decks by file stem or deck name, parses every config field type with a typed parser, and exposes reference `fast`, `standard` and `strong` as named agents.
- **Verification** (unit + e2e): the installed binary runs with the default deck set from any directory; every config field round-trips through the parser; output labels show deck names.
- **Status:** open

## Performance

### W-124 — Search is engine-bound; invalidating the derived view by mutation kind is deferred
- **Category:** performance · **Resolution type:** Rust fix (output-identical)
- **Sources:** `docs/AI.md:280-299`, `docs/AI.md:497-502`, `docs/AI.md:510`, `docs/ARCHITECTURE.md:50-51`, `docs/ARCHITECTURE.md:76-81`, `docs/ASSUMPTIONS.md:93`, `README.md:110-116`, `docs/status.json:48`, `src/gcg_sim/ai/config.py:93-97`, `d3f8dd6`, `c6716f5`, `d9d41af`
- **Quotes:** “Invalidating the derived view by mutation kind (74% of recomputations produce an identical view) needs a dependency analysis of every condition and is deferred.”; “In a profiled game on the pre-change engine, 80–83% of search time was inside the engine's `apply`”; “28.2 vs 2.90 (9.7x)”; “Engine performance: per-zone ability lists, single base view per derived computation, event-indexed triggers (1.7x throughput)”
- **What is weak:** games per core-minute: `fast` 28.2, `standard` 2.90 (20.7 core-s per game), `strong` 1.82, pre-change `standard` 4.72; 100 BO3 matches on 8 workers took 13.0 min. The speedups cut calls by about a third, view recomputations by 40% and determinizations by 73% (28,134 to 7,627 per game); the derived view is a fixpoint recomputed after any mutation and 74% of recomputations produce an identical view. Pre-#5 profile: 91% in `apply`, 69% in `view._compute`.
- **Resolution plan:** in Rust: (1) static dependency masks for every condition and filter computed from the IR (which state facets each reads), and invalidation by mutation kind, with conditions the analysis cannot see through (custom steps that call execute) marked "dynamic" and always recomputed; (2) arena-allocated, cache-friendly state with cheap clones; (3) the paranoid feature (W-49) compares every masked skip with a full recomputation.
- **Verification** (e2e + unit): output identity (games, decision logs, results) with masks on and off over the equivalence seeds; paranoid fuzz with zero mismatches; throughput measured in node-count-normalised units (W-126) against the Python baseline.
- **Status:** open

### W-125 — Action and Decision are frozen dataclasses (slow construction on the hot path)
- **Category:** performance · **Resolution type:** Rust fix (by construction)
- **Sources:** `src/gcg_sim/engine/state.py:14-15`, `src/gcg_sim/engine/state.py:35-36`, `tests/perf/__pycache__/test_engine_values.cpython-315-pytest-9.1.1.pyc`
- **Quotes:** “``Action`` and ``Decision`` are plain (not frozen) dataclasses for construction speed”
- **What is weak:** on this branch both are frozen dataclasses built in the hot path (the lost perf branch made them plain with value semantics and a fast `copy()` for `GameState.clone`).
- **Resolution plan:** in Rust `Action` is a small `Copy` value type and `Decision` holds its options in a reusable buffer; equality and hashing are by value.
- **Verification** (unit): equality semantics tests (two actions with the same fields are equal and hash equal); micro-benchmark recorded in the port's benchmark report.
- **Status:** open

### W-126 — Throughput measurement counts wall time x workers on one pairing; machine state is not recorded
- **Category:** performance · **Resolution type:** tooling
- **Sources:** `src/gcg_sim/ai/throughput.py:5`, `src/gcg_sim/ai/throughput.py:21`, `src/gcg_sim/ai/throughput.py:33-38`, `bench/__pycache__/timing.cpython-315.pyc`, `bench/perf/__pycache__/micro_compare.cpython-315.pyc`, `bench/perf/__pycache__/ab.cpython-315.pyc`
- **Quotes:** “core_minutes = wall * workers / 60.0”; “Runs are not gated on a quiet machine”
- **What is weak:** process spawn, imports and idle tail workers count as busy core time; only `TUNING_DECKS[0]` vs `[1]` is measured; the documented `--games 26` is not a multiple of the 4-game block; machine load and power state are not recorded; the lost harness used the minimum over runs and a 1.03 regression ratio.
- **Resolution plan:** the port's throughput tool (outside the deterministic crates, as `scripts/compare_python_vs_rust.sh` already is) reports CPU time per game from the OS, nodes and decisions per second (deterministic counts over measured time), several deck pairings, whole blocks only, the minimum and median of repeated runs, and the machine's load and power state.
- **Verification** (e2e): two runs on an idle machine agree within 3% on the minimum; a `--games` value that is not a multiple of 4 is rounded up with a message.
- **Status:** open

## Docs

### W-127 — Investigation evidence is not reproducible: raw outputs were gitignored and cited commits were squashed away
- **Category:** docs · **Resolution type:** docs + eval tooling
- **Sources:** `docs/PROGRESS.md:96-103`, `docs/PROGRESS.md:109-110`, `docs/PROGRESS.md:117`, `docs/PROGRESS.md:126`, `docs/PROGRESS.md:134-163`, `docs/AI.md:280`, `docs/AI.md:345`, `d9d41af`, `23ddd49`
- **Quotes:** “Raw outputs under `build/aiq2/` (gitignored).”; “Measured on commit fc29d5f”
- **What is weak:** `build/aiq2/` (arms files, audit transcripts, `classified.md`) is absent from this checkout; `git cat-file` fails for 5a6f226, fc29d5f, daf73c5, c27e523, ed2cedc, 4be24da, 87d8a72, 65d6e1d, 9684f29, 8eb9e8a, a339080, 3351b78, 39c5643, d79ea78, a32d212 and 4d8bd95 (verified this session), all squashed into d9d41af; 99 commits exist, all reachable from `main`, and the tag `baseline` points at d9d41af.
- **Resolution plan:** for the port, every number used as evidence is produced by a committed gcg-eval command whose raw output (game records, ablation rows, traces) is committed in a compressed evidence directory with the exact command and commit; Rust docs cite only reachable commits, checked by a doc test. The Python docs' dangling shas are listed for the lead (Python docs are outside the port's files).
- **Verification** (unit): a doc check resolves every commit sha in the port docs with `git cat-file -e` and every evidence file path exists.
- **Status:** open

### W-128 — Stale counts across status.json and the research docs
- **Category:** docs · **Resolution type:** docs (generated counts)
- **Sources:** `docs/status.json:3`, `docs/status.json:18-33`, `README.md:8`, `docs/CONFLICTS.md:90`, `docs/PROGRESS.md:20-21`, `docs/research/RULES_INDEX.md:24-25`, `docs/research/RULES_INDEX.md:45`, `docs/research/RULES_INDEX.md:79`, `docs/research/RULES_INDEX.md:86`, `docs/RULES_TRACEABILITY.md:5`, `docs/research/EFFECT_TEMPLATES.md:15`, `docs/research/EFFECT_TEMPLATES.md:19-23`, `docs/research/EFFECT_TEMPLATES.md:225`, `docs/research/work_packages.json:7-8`, `docs/research/DATA_INGESTION.md:7-9`, `docs/research/DATA_INGESTION.md:22-23`, `docs/research/DATA_INGESTION.md:145-147`, `docs/research/data_profile.json:2851`, `docs/research/data_profile.json:3478`, `docs/research/OFFICIAL_SOURCES.md:204`, `src/gcg_sim/data/official/edition_language.json:9`, `data/sources.d/gcgapi.json`
- **Quotes:** “"card_numbers": 1148”; “the 399 ids that need a @pytest.mark.rule test”; “At this snapshot there are 1,148 card numbers (1,912 printings).”; “"data: weekly refresh (run 25)"”; “Card data holds 85 Edition Beta printings.”
- **What is weak:** `status.json` (dated 2026-10-18) says 1148/1148 cards and 395 conflicts while README says 1152/1152 and CONFLICTS 402; RULES_INDEX says 399 testable / 180 N/A against traceability's 395 / 184 (7a797fb reclassified four) and says the CR PDF was kept only in the scratchpad; EFFECT_TEMPLATES prose says 1,148 numbers / 1,912 printings / 308 vanilla / 189 binding cards while its generated blocks and `work_packages.json` say 1,152; DATA_INGESTION and `data_profile.json` profile run 25 (1,912 printings) against the packaged run 28 (2,011); Edition Beta counts are 83 printings / 70 numbers vs 85.
- **Resolution plan:** every count in the port's docs is generated from the packaged data or test results by a docs tool and checked by a test (counts cannot be hand-typed); the list of stale Python-doc counts above goes to the lead for the Python side.
- **Verification** (unit): the port docs' counts test regenerates every count and fails on any difference.
- **Status:** open

### W-129 — Hand-maintained docs drift from the code
- **Category:** docs · **Resolution type:** docs (checked contracts)
- **Sources:** `docs/SPEC.md:18`, `pyproject.toml:9`, `docs/ASSUMPTIONS.md:75-78`, `docs/INTERFACES.md:64`, `docs/INTERFACES.md:143-149`, `docs/DECK_FORMAT.md:88`, `src/gcg_sim/deck/validate.py:268`, `docs/PROGRESS.md:136`, `src/gcg_sim/ai/selfplay.py:95-104`, `docs/AI.md:326-327`, `docs/AI.md:309-310`, `15ae2eb`, `32e9f84`, `99f43fe`, `ceb7d1d`
- **Quotes:** “Python ≥3.12”; “def validate_deck(deck: Deck, *, banlist: bool = True) -> list[Violation]”; “values still provisional”; “SPEC, CLAUDE.md, skill, DECK_FORMAT, REPORTS, AI and marker docs corrected”; “gcg-benchmark skill uses the real CLI”; “Blocks of four games on one game seed covering every (seat, deck) assignment”; “means blocks of four games per deal”
- **What is weak:** SPEC says Python ≥3.12 while the package requires ≥3.15. The "frozen" INTERFACES contract lacks `implemented=True` and omits `replay --log` and `data status --json`. PROGRESS still calls the `fast` values provisional. Review commits corrected doc drift repeatedly, and the benchmark skill was drafted before the CLI existed. The `mirrored_specs` docstring ("Blocks of four games on one game seed") and `docs/AI.md:326-327` ("blocks of four games per deal") are also wrong: the code derives the game seed from the block and the deck order, so each block of four holds two deals of two games each (W-42). `docs/AI.md:309-310` ("each deal with the challenger on both decks and in both seats") is correct. The error matters for any paired analysis (W-42, W-43).
- **Resolution plan:** the port's interface docs are generated from the Rust public API (`cargo doc` plus a CLI `--help` snapshot test), so signatures and subcommands cannot drift. The gcg-eval docs describe a mirrored block as two deals, each played with the agents swapped, and a test checks that description against the ported generator. The Python-doc corrections above, including the `mirrored_specs` docstring and `docs/AI.md:326-327`, go to the lead.
- **Verification** (unit): snapshot tests of `--help` output for every subcommand and of the public API signatures fail on any undocumented change. A unit test checks the ported mirrored-block generator: every block of four uses exactly two game seeds, each in two games with the agents in opposite seats, as the gcg-eval docs state, and the seeds equal Python's `mirrored_specs` on fixed master seeds.
- **Status:** open

### W-130 — The shuffle-cut and chooser-procedure wording disagree with the engine and REPORTS
- **Category:** docs · **Resolution type:** port-preserve + docs
- **Sources:** `docs/research/OFFICIAL_SOURCES.md:140-141`, `src/gcg_sim/engine/game.py:116-117`, `src/gcg_sim/data/official/floor_rules.json:34`, `docs/REPORTS.md:36`
- **Quotes:** “The confirming cut or shuffle is not modelled, because it leaves a uniform permutation uniform.”; “Seeded fair coin flip picks the chooser.”
- **What is weak:** the research doc says the confirming cut is not modelled, while `new_game` shuffles with the seeded RNG and then cuts at a random point (Floor Rules), and the RNG consumption order must match for replay; `floor_rules.json` says a seeded fair coin flip picks the chooser while REPORTS says a seeded die roll.
- **Resolution plan:** Rust reproduces the engine exactly (shuffle, then cut, same RNG draws) and its docs describe what the engine does; the wording mismatch in the Python docs and the packaged data file is listed for the lead (packaged data stays unchanged in the port).
- **Verification** (unit): RNG-stream test: Rust `new_game` consumes the same SplitMix64 draws as Python for a set of seeds (same deck order and uids).
- **Status:** open

## Design decisions

### W-131 — Multiplayer (rules section 12) is out of scope
- **Category:** design-decision · **Resolution type:** admissible exception
- **Sources:** `README.md:164`, `docs/SPEC.md:45`, `docs/ASSUMPTIONS.md:57`, `docs/research/RULES_INDEX.md:39`, `docs/research/RULES_INDEX.md:77`, `docs/research/RULES_INDEX.md:92`, `docs/research/OFFICIAL_SOURCES.md:48-50`, `docs/research/EFFECT_TEMPLATES.md:890-902`, `docs/CONFLICTS.md:5438-5450`, `src/gcg_sim/effects/compiler/grammar.py:8-10`, `src/gcg_sim/effects/compiler/grammar.py:66-67`, `src/gcg_sim/effects/bindings/wp_eb01_a.py:3`, `src/gcg_sim/effects/bindings/wp_eb01_a.py:334`, `src/gcg_sim/effects/bindings/wp_eb01_b.py:4-6`, `src/gcg_sim/rules/index.py:6`, `src/gcg_sim/data/rules/rules_na.json:516`, `tests/meta/rulings_na/wp_eb01_a.json`, `tests/meta/rulings_na/wp_st09_11.json`, `e240669`
- **Quotes:** “**1v1 only.** Multiplayer (rules section 12) is out of scope; its rules are marked N/A.”; “"2 or more enemy players" is false: EB01-040, EB01-044, EB01-055 and EB01-058 then do”; “1v1 interpretation of multiplayer wording (rule 12 is out of scope)”; “Multiplayer Battle | 32 | 1 | 31 | 32 | 0”
- **What is weak:** all 32 section-12 rules are N/A; multiplayer wording is read for 1v1 ("number of enemy players" = 1, "each enemy player"/"another player" = the opponent, ST10-001's "enemy team" = the opponent), so EB01-040/044/055/058 never fire; 8 rulings are N/A as multiplayer-only (EB01-005:Q312, EB01-009:Q314, EB01-074:Q330, GD05-034:Q486, ST10-001:Q302, ST10-008:Q305, ST10-008:Q306, ST14-001:Q464); the multiplayer formats Team Battle and Battle Royale are unsupported. Sealed Battle is a two-player limited deck-construction format, not multiplayer, so it is not part of this exception (W-76).
- **Resolution plan:** none required: an explicit design decision of the reference. The port keeps the 1v1 readings exactly, and the multiplayer N/A list is ported as data so the traceability report stays complete.
- **Verification** (n/a): exception; the 1v1 readings are covered by the ported tagged tests (W-56).
- **Status:** admissible exception (design decision): the reference simulates two-player Standard play only; multiplayer was explicitly scoped out.

### W-132 — No sideboarding in BO3
- **Category:** design-decision · **Resolution type:** admissible exception
- **Sources:** `README.md:53`, `README.md:165`, `docs/research/OFFICIAL_SOURCES.md:174-176`, `docs/research/OFFICIAL_SOURCES.md:255-259`, `docs/CONFLICTS.md:966-973`, `docs/DECK_FORMAT.md:72-73`, `src/gcg_sim/data/official/banlist.json:30`, `src/gcg_sim/data/official/banlist.json:34`, `src/gcg_sim/data/official/deck_construction.json:118`, `src/gcg_sim/data/official/bo3_match_rules.json:19`, `src/gcg_sim/runner/match.py:19-37`
- **Quotes:** “**No sideboarding or clock** in BO3, as the task specifies.”; “the simulator uses the main deck only; no sideboard”
- **What is weak:** the official BO3 page allows a 10-card sideboard (4-copy and 2-colour limits count across deck and sideboard; the TRM says "No side decks are permitted"); the simulator ignores both, plays the same deck every game, and applies B&R and banned pairs to the main deck only.
- **Resolution plan:** none required: an explicit design decision of the reference ("as the task specifies").
- **Verification** (n/a): exception.
- **Status:** admissible exception (design decision): BO3 is modelled without sideboarding by specification.

### W-133 — No clock: time limits, extra turns and tiebreaks are not modelled
- **Category:** design-decision · **Resolution type:** admissible exception
- **Sources:** `README.md:165`, `docs/research/OFFICIAL_SOURCES.md:169-178`, `docs/research/OFFICIAL_SOURCES.md:233`, `.claude/skills/gcg-refresh-data/references/sources.md:74`, `src/gcg_sim/data/official/bo3_match_rules.json:16-21`, `src/gcg_sim/data/official/floor_rules.json:4`, `src/gcg_sim/data/official/floor_rules.json:128-134`, `src/gcg_sim/runner/match.py:19-37`, `src/gcg_sim/runner/game.py:42-51`
- **Quotes:** “The simulator also ignores time limits and extra turns.”; “no clock in the simulator.”
- **What is weak:** officially a BO3 has 60 minutes, then 3 extra turns if time runs out in Player One's turn or 2 in Player Two's, then tiebreaks (the Asia floor rules end with rock-paper-scissors). Time-related penalties are not modelled either, and a match ends after at most `games_per_match` games. Intentional draws, concessions and other penalties are tournament procedures, not part of the clock, so they are covered by W-76, not by this exception.
- **Resolution plan:** none required: an explicit design decision of the reference. Games are bounded by the engine safety caps instead (W-46).
- **Verification** (n/a): exception.
- **Status:** admissible exception (design decision): simulated games have no wall-clock dimension by design (and budgets and determinism forbid one).

### W-134 — Both players know both exact decklists
- **Category:** design-decision · **Resolution type:** AI (strengthened search, opt-in information mode)
- **Sources:** `docs/ASSUMPTIONS.md:62`, `docs/ASSUMPTIONS.md:90`, `docs/AI.md:30-31`, `docs/AI.md:200`, `docs/ARCHITECTURE.md:88-91`, `src/gcg_sim/engine/observe.py:3`, `32e9f84`
- **Quotes:** “Both AI players know both exact decklists (main and resource deck)”; “Both decklists are known to both players (a benchmarking assumption, docs/ASSUMPTIONS.md);”; “decklist knowledge recorded as an assumption”
- **What is weak:** a benchmarking choice, not a game rule (the floor rules keep main-deck contents private); determinization resamples hidden cards from the opponent's decklist minus seen cards and the `bursts` feature reads the known decklist, so the AI plays with information a real player may lack (the benchmark deck is a known meta list, the deck under test may not be).
- **Resolution plan:** keep the default (and the reference) unchanged. Add an opt-in information mode to the strengthened search, `--opponent-list known|archetype`: in `archetype` mode the opponent's unseen cards are sampled from a declared prior (a list of candidate decklists with weights, e.g. the benchmark deck's archetype variants) updated by the cards seen (Bayesian reweighting of candidate lists), never from the true decklist. AI-only; rules and data unchanged.
- **Verification** (unit + e2e): information-set test in `archetype` mode: replacing the opponent's true decklist by another list consistent with the prior and the seen cards does not change the decision or statistics; e2e: a benchmark in `archetype` mode with a one-list prior equal to the true list reproduces `known` mode exactly.
- **Status:** open

### W-135 — Information-set honesty is a convention, not enforced by the API
- **Category:** design-decision · **Resolution type:** Rust fix (by construction)
- **Sources:** `docs/INTERFACES.md:29-32`, `docs/SPEC.md:77-78`, `docs/AI.md:34-55`
- **Quotes:** “agents receive the full state and read only their own information set”
- **What is weak:** agents receive the true `GameState`; honesty is checked by permutation tests (25 mid-game states from greedy self-play, 3 permutations each, plus a negative control), not by types.
- **Resolution plan:** Rust agents receive an `Observation<'_>` view that exposes only the deciding player's information set (public zones, own hand, counts, known cards, own pending options) and a `determinize` method; the reference ISMCTS and greedy are ported against this view, which offers the same information, so their outputs stay identical. Only the engine and the runner hold `&GameState`.
- **Verification** (unit + compile-fail): agent code cannot name `GameState` (compile-fail test); the ported permutation test and negative control pass; reference outputs equal Python's on the equivalence seeds.
- **Status:** open

### W-136 — SearchConfig defaults reproduce the pre-change AI; baseline presets live in code
- **Category:** design-decision · **Resolution type:** Rust fix (separate types)
- **Sources:** `src/gcg_sim/ai/config.py:27-46`, `docs/ASSUMPTIONS.md:91`, `docs/AI.md:274-278`, `docs/AI.md:321-322`
- **Quotes:** “every new `SearchConfig` field defaults to the old behaviour”
- **What is weak:** `value_scale` defaults to 2.0 and every new knob is off; the presets opt in through `adopted()`; `BASELINE_PRESETS`/`BASELINE_WEIGHTS` exist so `tests/ai/test_baseline.py` can replay a bc38ab4 game bit for bit. A config built from defaults silently gets the old AI.
- **Resolution plan:** in Rust the reference search has its own config type with the Python fields and defaults, reachable only through gcg-eval; the strengthened search has a separate config type with no defaults (every preset spells out every field), so neither can be built by accident from the other.
- **Verification** (unit): the bc38ab4 baseline game and the d9d41af presets replay bit for bit through gcg-eval; constructing a strengthened config without every field does not compile.
- **Status:** open

### W-137 — The determinism contract; cross-platform identity depends on the C math library
- **Category:** design-decision · **Resolution type:** port-preserve + tests + portable math (strengthened search); the `erfc` FFI needs the lead's decision
- **Sources:** `CLAUDE.md:32-33`, `docs/ARCHITECTURE.md:106-111`, `docs/AI.md:23-26`, `docs/INTERFACES.md:5`, `docs/SPEC.md:44`, `src/gcg_sim/ai/policy.py:98`, `src/gcg_sim/ai/evaluation.py:303-307`, `src/gcg_sim/ai/mcts.py:199`, `src/gcg_sim/ai/mcts.py:297`, `src/gcg_sim/ai/mcts.py:314`, `src/gcg_sim/ai/setup.py:109`, `src/gcg_sim/reports/stats.py:60`, `docs/port/inventory/09-python-hazards.md:421-437`, `docs/port/inventory/09-python-hazards.md:444-459`, `docs/port/inventory/09-python-hazards.md:855-864`, `Cargo.toml:17-18`, `port/oracle/export_vectors.py:1-8`
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
- **Status:** open

### W-138 — Multi-card choices are sequential single picks
- **Category:** design-decision · **Resolution type:** port-preserve + AI (strengthened search)
- **Sources:** `docs/ARCHITECTURE.md:53-56`, `docs/INTERFACES.md:21-28`, `src/gcg_sim/ai/actions.py:41-45`
- **Quotes:** “Multi-card choices are sequential single picks with a DONE option”
- **What is weak:** picks are ordered so each subset is reachable once; this shapes the search's branching factor (one node per partial subset) and the replay action encoding (`Action(kind, a, b, c)` with the EX count packed into `c` modulo `ALT_PLAY_BASE`).
- **Resolution plan:** the engine and replay encoding stay exactly as in Python. The strengthened search may treat a sequence of single picks of one choice as one macro-move (the chosen subset) at its own nodes, expanded back into the engine's single picks when applied, so subsets share statistics without changing the engine interface.
- **Verification** (unit + ablation): replays recorded by the strengthened search are valid sequences of single picks that the reference engine accepts; ablation row for macro-moves.
- **Status:** open

## Later additions

Two completeness reviews found these entries after the first numbering: W-139 to W-154, then
W-155 to W-164. Each keeps its own category, and the IDs continue the sequence, so earlier IDs
stay stable.

### W-139 — Opponent nodes are tuned against the searcher's true private cards
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/mcts.py:1-12`, `src/gcg_sim/engine/observe.py:56-83`, `src/gcg_sim/ai/evaluation.py:178-188`, `docs/AI.md:462-469`
- **Quotes:** “One tree is built from the deciding player's point of view.”; “Return a full game state consistent with ``observer``'s information set.”
- **What is weak:** `determinize` redraws only the cards the observer cannot identify, so the searcher's own hand, and every card it has seen, is the same in every world. Three things are computed against the searcher's real cards: the opponent nodes' statistics, the opponent's rollout choices, and the leaf evaluation (whose hand counts and castable-removal "answers" read both hands). In the tree, the opponent effectively plays around tricks the searcher really holds and is never bluffed. The search therefore undervalues holding a trick, and any line that relies on the opponent not knowing. This is inferred from the design and has not been measured. W-05 covers the other direction: opponent nodes pooling statistics over the opponent's own hidden cards.
- **Resolution plan:** strengthened search, partial multiple-observer ISMCTS. A fixed share of each search's iterations (chosen by ablation, starting at 1 in 4, counted in the node budget) is played in "opponent-view" worlds. In those worlds, the searcher's private cards (its hand and every other card the opponent cannot identify) are redrawn from the opponent's information set. Opponent nodes select moves by the statistics of those iterations only, falling back to all iterations below a minimum visit count. The searcher's own nodes keep the normal iterations. This combines with W-05's buckets and W-07's belief weights. The reference is unchanged.
- **Verification** (scenario + ablation): scenario "held trick". The searcher holds an action-step AP−3 Command and the Resources to cast it, and the opponent can attack the searcher's Unit. With the trick, the Unit survives and wins that battle. The ground truth is fixed by 2,000 paired simulations against a reference `standard` opponent that plays from its own information set: keeping the Resources open beats spending them in the main phase. Over 16 seeds, the reference keeps the trick in at most 8 (recorded; its opponent nodes learn not to attack, so holding looks worthless), and strengthened `standard` keeps it in at least 12. Ablation row alone and leave-one-out.
- **Status:** open

### W-140 — In-tree priors come from the first world that reaches each node
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/mcts.py:128-130`, `src/gcg_sim/ai/mcts.py:281-282`, `src/gcg_sim/ai/mcts.py:305-317`, `src/gcg_sim/ai/policy.py:133-156`, `docs/AI.md:97-100`
- **Quotes:** “_add_children(root, first_world, dec, root_moves, config)”; “node.children.setdefault(_child_key(dec, key, cfg), _Child(dec.player, wgt / total))”; “Priors are the playout policy's scores through a softmax (temperature 1.5).”
- **What is weak:** a node's priors are the softmax of `policy.option_scores`, computed in one world: the root's in world 0, and each in-tree node's in the world of the iteration that first adds its children. When a later world offers a move the node has not seen, `_add_children` scores that world's whole move set, but `setdefault` keeps the earlier priors. A node's priors then mix softmaxes normalised over different move sets. At opponent nodes the moves themselves (the cards of a sampled hand) depend on the world, so one sampled hand can set the priors. At the root the moves are the true decision's, and the MAIN scorer's board summary reads only public state (`policy.py:133-156`), so root priors probably do not depend on the world. However, no test checks that every scorer reads only the decider's information. The priors stay honest about information, but they are noisy. No existing entry covers this.
- **Resolution plan:** strengthened search: an in-tree node's prior for each move averages the policy scores over the first K worlds that reach the node (K = 4, the common-random-number worlds). The priors are normalised over the union of moves seen, and recomputed when a new move appears. The root prior is computed once. The reference keeps first-world priors.
- **Verification** (unit + ablation): a unit test shows that a strengthened node's priors are the same whatever order the same K worlds arrive in. A second unit test, on 1,000 recorded positions, checks that root priors are equal across 8 determinizations for every decision kind. The reference result is recorded, and any difference shows which scorer reads hidden state. Ablation row alone and leave-one-out.
- **Status:** open

### W-141 — The no-effect branch check compares a branch with a policy line that may replay the move
- **Category:** ai-search · **Resolution type:** AI (strengthened search)
- **Sources:** `src/gcg_sim/ai/noop.py:13-17`, `src/gcg_sim/ai/noop.py:154-165`, `src/gcg_sim/ai/noop.py:197-210`, `src/gcg_sim/ai/config.py:68`, `docs/port/inventory/07-ai-search.md:1071-1073`
- **Quotes:** “idled = lines.world.clone()”; “which may replay the move under test”
- **What is weak:** `noop_branch_choices` is on in the presets. With it, a move judged without effect is re-checked once per option of its first choice, and kept when some branch "beats idling". When a branch's replay diverges, the idle baseline becomes the deterministic policy continued from the world without the move (`noop.py:203-205`). That policy still holds the card and may cast the same Command or activate the same ability. The comparison is then "this branch against the policy's own line", not "the move against idling". A useful branch that is no better than the policy's own use of the move does not count, so the move can be pruned. The port inventory flags this as an open question, and W-12 does not cover it.
- **Resolution plan:** the reference keeps it bit-exact. The strengthened no-effect check builds the idle line with the move under test removed from the options of every decision in the continuation. "The move" covers every variant of the same card's play or activation. The baseline is then real idling.
- **Verification** (unit + ablation): a testkit position where the deterministic policy, continued without the move, casts the same Command. The reference prunes the move (pinned). The strengthened check keeps it, and its idle line contains no action of that card. Included in the bundle's leave-one-out row with W-12.
- **Status:** open

### W-142 — The evaluation is fitted on true-state features but scores determinized states; Burst density is computed once per search
- **Category:** ai-evaluation · **Resolution type:** AI (strengthened search) + eval tooling
- **Sources:** `src/gcg_sim/ai/tuning.py:148-157`, `src/gcg_sim/ai/mcts.py:113-120`, `src/gcg_sim/ai/mcts.py:263`, `src/gcg_sim/ai/evaluation.py:132-140`, `src/gcg_sim/ai/evaluation.py:178-188`, `docs/AI.md:167-169`, `docs/port/inventory/08-ai-policy-eval.md:800-806`
- **Quotes:** “density = burst_density(st, st.active)”; “states only, so the opponent's hand composition is a sample from the player's information”; “in both cases with `density = burst_density(st, st.active)` on the TRUE state.”
- **What is weak:** tuning records features at the first MAIN decision of each turn, on the true game state. That includes both players' real hands (hand counts by card type, and castable-removal "answers"), with Burst density from the active player's view. The search scores determinized worlds instead, where the opponent's hand is a uniform sample from its unseen cards (W-07), not the cards that player chose to keep. So the weights are fitted on one feature distribution and used on another. In addition, the density used at every leaf is computed once, at the root, from the searcher's view (`mcts.py:117`). It is reused even when the leaf is two turns later, when its turn player differs, or when Shields and unseen cards have changed. Neither effect has been measured, and W-34 to W-40 do not mention them.
- **Resolution plan:** the gcg-eval `tune` tool (W-34, W-40) records each position's features in a determinization from the recorded player's view. It uses the same feature and density functions the search calls (one shared code path). It reports log-loss for true-state and determinized recording side by side, and the strengthened presets use whichever fit is better out of fold. The strengthened leaf recomputes Burst density from the searcher's known cards in the leaf world. The reference keeps both behaviours.
- **Verification** (unit + ablation): a unit test shows that, on 100 positions, the tuning tool's determinized features equal the search's leaf features for the same world and seed. Log-loss table for both recording modes (out of fold by game, and on the held-out panel decks). Ablation rows for the determinized fit and for per-leaf density, alone and leave-one-out.
- **Status:** open

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
- **Status:** open

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
- **Status:** open

### W-145 — The packaged card data predates GD06 (released 2026-10-30), so its cards are unknown
- **Category:** data · **Resolution type:** Rust fix (data-driven loader) + reporting
- **Sources:** `src/gcg_sim/data/gcgapi/products.json:1`, `src/gcg_sim/data/gcgapi/manifest.json:3-5`, `README.md:130`, `src/gcg_sim/deck/parse.py:57-61`
- **Quotes:** “"set_code":"GD06","release_date":"2026-10-30"”; “"built_at": "2026-10-12T14:37:38.154Z",”; “unknown card id {written!r} (not in the card data)”
- **What is weak:** the packaged snapshot (gcg-api dataset 28, built 2026-10-12) lists two newer sets in `products.json`: GD06 Stardust Trails (released 2026-10-30) and GD07 Blazing Fist (due 2027-01-29). But `cards.ndjson` has no GD06 card number (0, counted this session). A deck with GD06 cards fails with UNKNOWN_ID, so benchmark decks from the post-GD06 meta cannot be modelled. Neither `data status` nor the reports say that a released set is missing, and no entry recorded how old the data is. Refreshing the data is a Python-workflow task (`/gcg-refresh-data`), and implementing the new cards is W-63's binding work.
- **Resolution plan:** no data change in the port. The Rust data crate loads a refreshed snapshot without code changes: sets, products and card numbers are data, never enums or constants. An opt-in `gcg-sim data status --freshness [--as-of DATE]` lists every product released after the snapshot's `built_at` that has no card numbers in the data. The flag is opt-in, so the default output stays equal to Python's. Running `/gcg-refresh-data` for GD06 is handed to the lead.
- **Verification** (unit + e2e): unit: a synthetic snapshot copy adds a new set (set code, product entry and ten vanilla card numbers). It loads, and a deck using the new cards validates, with no Rust code change. e2e: with the packaged data, `data status --freshness --as-of 2026-11-01` names GD06 as released on 2026-10-30 with 0 card numbers, and does not list GD07, which is not yet released.
- **Status:** open

### W-146 — The rules markdown is checked against the official PDF only by its set of rule ids
- **Category:** data · **Resolution type:** data tripwire
- **Sources:** `.claude/skills/gcg-refresh-data/scripts/official_normalize.py:110-147`, `.claude/skills/gcg-refresh-data/SKILL.md:122-124`, `.claude/skills/gcg-refresh-data/SKILL.md:128-139`, `docs/research/OFFICIAL_SOURCES.md:219-221`, `data/official_raw/en_pdf_comprehensiverules_en.txt`
- **Quotes:** “`official_normalize.py check` (step 6) proves that the PDF and the markdown have the same”; “Update `src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md` from the PDF text.”; “Section 6 was compared word”
- **What is weak:** the markdown is written by hand from the PDF text whenever the rules change (step 2 of the new-version procedure). `check_rules` compares the version and date, the root copy's hash, the PDF hash and the sets of rule ids (566 in each), but never the rule text. Only section 6 was ever compared word for word. A transcription error in any other rule (a dropped "not", a wrong number or cross-reference) would pass every gate, even though traceability and the engine's interpretations cite that text. W-87 covers errors in the official file itself, and W-88 the omitted revision history; neither covers transcription fidelity.
- **Resolution plan:** a Rust data-gate test normalises both texts, then compares each rule's text in the markdown with the same rule in the cached PDF text extraction (`data/official_raw/en_pdf_comprehensiverules_en.txt`), listing every difference. Normalisation covers whitespace and line wrapping, the markdown escapes (`\<`, `\[`), the bold rule numbers and the `> Ex:` prefixes. Known PDF-extraction artefacts (ligatures, page headers and footers, hyphenation) go in an allowlist in the port's test data, with each entry naming its rule. The cached sources stay unedited. A real transcription error is handed to the lead for `/gcg-refresh-data`.
- **Verification** (unit + negative control): the test passes on the packaged files with a reviewed allowlist, whose size is reported. As a negative control, changing one word of one rule in a temporary copy of the markdown fails the test with that rule id.
- **Status:** open

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
- **Status:** open

### W-148 — Saved replays are median-length games, and the CLI cannot replay other games.ndjson records
- **Category:** reporting · **Resolution type:** Rust fix (opt-in)
- **Sources:** `src/gcg_sim/reports/results.py:86-90`, `docs/REPORTS.md:116-118`, `.claude/skills/gcg-benchmark/SKILL.md:114-117`, `src/gcg_sim/cli.py:92-94`
- **Quotes:** “Representative games to save: wins and losses near their median length, plus a draw.”; “Check a few replays of losses for implausible plays before trusting a surprising”; “can be replayed the same way with”
- **What is weak:** `select_replays` saves `--replays` wins and losses nearest their median length, plus at most one draw. The benchmark skill tells users to check replays of losses for implausible plays. But the games that most need checking (very fast losses, surprising wins, games near the safety cap) are rarely saved. `gcg-sim replay` takes only replay JSON files, so any other `games.ndjson` record can be replayed only through the Python API (`gcg_sim.runner.replay(record, config)`).
- **Resolution plan:** additive, with the default outputs unchanged. `gcg-sim replay --games-ndjson FILE --results RESULTS --match M --game G` re-simulates any recorded game from the run's configuration, verifies it and prints its log. `benchmark --replays-extremes` also saves the shortest and longest win and loss, under distinct file names, and only when the flag is given.
- **Verification** (e2e): for every game of a small run, `replay --games-ndjson` matches the recorded outcome and final digest. Without the new flag, the replay directory and every report are byte-identical to Python's. With it, the extra files are the games of minimum and maximum length, with ties broken by match index.
- **Status:** open

### W-149 — Which failure a parallel run reports depends on completion timing
- **Category:** runner · **Resolution type:** Rust fix (error path only)
- **Sources:** `src/gcg_sim/runner/run.py:45-53`, `src/gcg_sim/runner/run.py:67-80`, `src/gcg_sim/runner/run.py:88-106`, `docs/port/inventory/01-rng-runner-cli.md:1040-1042`
- **Quotes:** “In parallel runs, which failure gets reported when several matches fail depends on timing”; “done, pending = wait(pending, return_when=FIRST_COMPLETED)”
- **What is weak:** `_collect` waits for the next batch of completed matches, then calls `fut.result()` in index order within that batch. So the failure that gets raised, whose message names the match, game and seed, is the lowest failing index among the matches that happened to finish first. The serial path always reports the lowest failing match index. The error message, and so what a user investigates, depends on scheduling. That is an exception to the determinism contract, though on the error path only. W-105 and W-106 do not cover it.
- **Resolution plan:** on a failure at match k, the Rust runner cancels the matches above k (with W-106's cancellation flag) and lets the matches below k finish. It then reports the lowest failing index, which is the failure the serial path reports. Successful runs are unaffected.
- **Verification** (unit): a test agent that fails in matches 3 and 5 gives the match-3 error with 1, 2 and 8 workers, over 50 repetitions. With only match 5 failing, matches 0 to 4 finish before the error is reported.
- **Status:** open

### W-150 — Determinism acceptance tests run only with the random test agent
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `tests/cli/conftest.py:31-33`, `tests/cli/test_console_script.py:64-72`, `tests/runner/test_pool.py:18-25`, `scripts/verify_wheel_offline.sh:34-38`, `docs/SPEC.md:52`, `docs/status.json:6`, `docs/port/inventory/09-python-hazards.md:869-873`
- **Quotes:** “return {**os.environ, AGENT_FACTORY_ENV: RANDOM_AGENT}”; “def test_results_do_not_depend_on_the_python_hash_seed(”; “Neither covers the”
- **What is weak:** criterion 2's tests use the random test agent. That covers worker-count identity (`test_pool.py`, `test_console_script.py`) and the PYTHONHASHSEED check, which is the only test that varies the hash seed. The real search AI's worker-count identity is checked only by `verify_wheel_offline.sh`: one pairing, 2 matches, 1 vs 2 workers, `standard`. No test runs MCTS under different hash seeds. The port inventory's own hash-salt runs covered greedy (4 games) and `fast` (1 game), not `standard` or `strong`. A hash-order dependence in AI code, such as iterating a set of strings, would pass the gate. W-118 notes the small offline check but not the hash-seed gap.
- **Resolution plan:** Rust e2e tests run the `fast` and `standard` presets (reference and strengthened) with 1, 4 and 10 workers on two pairings, and compare every deterministic output byte for byte. On the Python side, the equivalence corpus is generated twice, under PYTHONHASHSEED=1 and PYTHONHASHSEED=2, with all presets. The two runs must be byte-identical before Rust is compared against them, and a mismatch is a Python-side bug handed to the lead. Rust has no hash seed of its own to vary, because W-137's lint forbids `HashMap` iteration.
- **Verification** (e2e + negative control): the worker-count tests pass, and the two Python generations are byte-identical (recorded in the port's progress log). As a negative control, a test-only agent that breaks ties by iterating a std `HashMap` (randomly seeded) makes the comparison fail.
- **Status:** open

### W-151 — Fuzz coverage counts cards in decks, not cards executed; tokens and EX cards are never in decks; the seed is fixed
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `src/gcg_sim/tools/fuzz.py:1-6`, `src/gcg_sim/tools/fuzz.py:33-43`, `src/gcg_sim/tools/fuzz.py:88-89`, `src/gcg_sim/tools/fuzz.py:132`, `src/gcg_sim/tools/fuzz.py:144-158`, `tests/slow/test_robustness.py:15-24`, `docs/SPEC.md:57`
- **Quotes:** “Generates decks that together include every implemented main-deck card”; “res.cards_seen.update(o["cards"])”; “res = run(GAMES, seed=2026, workers=workers)”
- **What is weak:** criterion 6 asks that the generated decks jointly include every card. The fuzz pool holds 956 coloured main-deck cards plus 108 Resources: 1,064 of the 1,152 card numbers (counted this session). The other 88 (29 UNIT TOKEN, 30 EX BASE, 29 EX RESOURCE) can never be in a deck. They appear only when an effect creates them, and nothing reports whether they did. `cards_seen` is deck membership, so a card that is never drawn, played or triggered still counts as covered. The 10,000-game test always uses seed 2026, so CI never explores new games. W-110 covers the agent choice and W-111 the tags; neither covers this.
- **Resolution plan:** Rust fuzz records, for each card number, whether its program ran (a step executed, a trigger fired or a static applied). Cards from decks and cards created by effects (tokens, EX Base, EX Resource) are recorded separately. The summary lists every pool card with a program that never ran, and each of the 88 effect-only numbers with its creation count. CI keeps the pinned seed 2026, for equivalence with Python's fuzz output. It adds a job whose seed is derived from the commit with `derive_seed` and printed, so any failure reproduces.
- **Verification** (unit + e2e): unit: a card that is in a deck but never drawn is reported as not executed. e2e: the 10,000-game run reports an execution status for every pool card, and every card with a program that never ran is listed with the tagged test that covers it. Each of the 88 effect-only numbers is either created in fuzz or covered by a golden record and a tagged test (W-117).
- **Status:** open

### W-152 — Seat and first-player symmetry was checked once and has no regression test
- **Category:** tooling · **Resolution type:** tooling
- **Sources:** `docs/PROGRESS.md:69-70`, `src/gcg_sim/runner/config.py:64-66`
- **Quotes:** “AI mirror 96 BO1 games, seat 0 won 50.”; “The deck under test sits in seat ``match_index % 2``, so seats alternate by match.”
- **What is weak:** seat symmetry was checked once, by hand. The check used 4,000 random-agent mirror games (first-player win rate 0.551 from seat 0, 0.539 from seat 1) and 96 AI mirror BO1 games (seat 0 won 50). No test in `tests/` checks it. Benchmark reports alternate the deck under test's seat by match, which would average away a seat-dependent engine bug (uid order, owner-ordered placement, the order of RNG draws).
- **Resolution plan:** a Rust e2e test plays mirror-deck games, with the same decklist in both seats, on fixed seeds: 4,000 random-agent games and 400 `fast` games. It asserts that the Wilson 95% interval of the seat-0 win rate contains 0.5. It also asserts that the first-player win rate does not differ by seat (two-proportion test, α = 0.01).
- **Verification** (e2e + negative control): the test passes on the Rust engine. As a negative control, a test-only engine build that gives seat 0 two extra Shields fails it.
- **Status:** open

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
- **Status:** open

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
- **Status:** open

### W-155 — Change detection ignores edits inside stripped parentheticals, so a reworded semantic parenthetical passes the refresh diff, the golden check and coverage
- **Category:** tooling · **Resolution type:** data tripwire + tooling (Python side, handed to the lead)
- **Sources:** `src/gcg_sim/tools/refresh.py:206-221`, `src/gcg_sim/tools/refresh.py:865-869`, `src/gcg_sim/effects/text.py:17-27`, `src/gcg_sim/effects/text.py:67-95`, `src/gcg_sim/effects/golden.py:40-41`, `.claude/skills/gcg-refresh-data/references/card-checklist.md:122`, `.claude/skills/gcg-refresh-data/SKILL.md:219-223`
- **Quotes:** “A `typography` change needs no work: the golden `text_hash` stays the same.”; “"kind": "typography" if old_norm == new_norm else "wording",”
- **What is weak:** `classify_effect` calls an effect edit `typography` whenever `normalize()` returns the same text, and the refresh skill says such a change needs no work. `normalize` strips every top-level parenthetical that `_is_reminder` accepts: one that ends with "." or has four or more words, and is not a trait. `text_hash` hashes the normalized text. The golden record's `text_hash` and the coverage report's TEXT CHANGED check (`refresh.py:865-869`) use the same hash, and the compiler reads the normalized text, so `golden --check` passes too. But three stripped parentheticals carry meaning that the bindings implement (W-60): GD05-089 "(Don't treat it as a Pilot.)", ST12-013 and ST14-014. Reproduced this session: rewording GD05-089's parenthetical to "(Treat it as a Pilot as well.)" leaves `normalize` and `text_hash` unchanged, and `classify_effect` returns `typography`. An erratum that rewords one of them would be reported as "no behaviour change", and the binding would go stale unnoticed. W-60's planned gate checks which cards have such parentheticals, not what they say.
- **Resolution plan:** a Rust data-gate test extracts every parenthetical that `normalize` strips. A scan this session found 16 distinct texts after normalisation. Thirteen restate a keyword or a card-type rule: the keyword reminders (one is the old <Breach> wording), and the Resource, EX Resource and EX Base rule texts (one, on EXRP-001, is a variant of the EX Resource setup text). The other three are exactly W-60's. The 13 go into a reviewed allowlist in the port's test data, each entry naming its keyword or rule. Every other stripped parenthetical is stored verbatim per card in a snapshot. The gate fails when a card gains a stripped parenthetical that is neither allowlisted nor in its snapshot, or when a snapshot text changes, and it prints the old and new text. On the Python side, a `parenthetical: review` class for such edits in the refresh diff is handed to the lead.
- **Verification** (unit + negative control): the gate passes on the packaged data, with a snapshot of exactly GD05-089, ST12-013 and ST14-014. In a temporary data copy, rewording GD05-089's parenthetical fails the gate, naming GD05-089. A typographic change to a keyword reminder (a curly apostrophe, extra spaces), which `normalize` removes, still passes. A keyword reminder reworded to a text that is not in the allowlist fails, so the new wording is reviewed once.
- **Status:** open

### W-156 — Replay files and games.ndjson records carry no data, rules or code version
- **Category:** reporting · **Resolution type:** Rust fix (diagnostics, output bytes unchanged)
- **Sources:** `src/gcg_sim/runner/replayfile.py:21`, `src/gcg_sim/runner/replayfile.py:104-148`, `src/gcg_sim/runner/replayfile.py:164-200`, `src/gcg_sim/runner/records.py:79-101`, `src/gcg_sim/reports/writer.py:54-57`, `src/gcg_sim/reports/versions.py:56-68`, `src/gcg_sim/cli.py:92-94`, `docs/REPORTS.md:106-108`
- **Quotes:** “Each replay is self-contained. It holds both decklists, the game seed, the chooser, and the”; “"version": REPLAY_VERSION,”
- **What is weak:** `replay_document` writes the document format and `REPLAY_VERSION = 1` (the version of that format), the seeds, decklists, agents, actions, the expected outcome and the log. It writes no `dataset_version`, rules version, B&R date, `code_digest` or package version. `GameRecord`, which is the line format of `games.ndjson`, has none of these either. Only `results.json` records versions (`versions.py:56-68`). After a data refresh, an override edit or an engine fix, `check_replay` fails with "action N … is not legal in the replayed game" or a `final_state_sha256` mismatch, and nothing says that the versions differ. A user cannot tell a regression from an expected change. W-89 and W-91 cover `results.json` only, and W-148 covers which games are saved.
- **Resolution plan:** replay and `games.ndjson` bytes stay unchanged (equivalence). The Rust `gcg-sim replay` looks for the run's `results.json`: one directory above `replays/`, or a path given with `--results`. It compares that file's `versions` with the running build's and names every difference (field, recorded value, current value). On a failed check, the message says whether the versions differ (an expected change is possible) or match (a regression or nondeterminism). Without a `results.json`, it says that the versions are unknown. When the check passes and the versions match, the output is what Python prints. W-148's `replay --games-ndjson`, which already takes `--results`, uses the same comparison.
- **Verification** (unit + e2e): a replay recorded on a synthetic data copy with a changed `dataset_version`, checked against the packaged data, fails and names the `dataset_version` difference. A replay checked with matching versions passes with Python's output. A replay file copied out of its run directory reports that the versions are unknown. Replay and `games.ndjson` bytes equal Python's.
- **Status:** open

### W-157 — The card database silently loads with no overrides when overrides.json is missing
- **Category:** data · **Resolution type:** Rust fix (fail fast)
- **Sources:** `src/gcg_sim/cards/db.py:62-68`, `src/gcg_sim/cards/db.py:328-329`, `src/gcg_sim/tools/explain.py:26-30`
- **Quotes:** “return {"schema_version": 1, "policies": {}, "resolutions": {}}”
- **What is weak:** `get_card_db()` builds `CardDB(load_raw_printings(), load_overrides())`, and `load_overrides` returns an empty resolution set when `overrides.json` is not found. If the packaged file were absent (after a packaging error, for example), all 402 resolutions (counted this session) would silently stop applying. They include the ST12-001 errata field override (W-79), the GD02-053, GD03-015 and GD03-035 text fixes (W-80), the null-AP fills (W-82) and the canonical-printing choices (W-83, W-86), so card behaviour would change with no error. `explain._conflicts` falls back the same way. The path is dead today, because the file is packaged and hashed in the lock.
- **Resolution plan:** the Rust loader treats a missing, unreadable or unparsable overrides file as a hard error that names the file. There is no empty default. The data gate's lock check (W-85) also fails on a missing file.
- **Verification** (unit): in a temporary data copy without `overrides.json`, loading fails with an error naming the file. A copy with truncated JSON fails with a parse error naming it. The packaged data loads 402 resolutions.
- **Status:** open

### W-158 — A damage-prevention threshold that is not a literal integer is silently read as unlimited prevention
- **Category:** engine · **Resolution type:** Rust fix (fail at registry build) + data tripwire
- **Sources:** `src/gcg_sim/engine/core.py:503-530`, `src/gcg_sim/effects/dsl.py:631-634`
- **Quotes:** “limit = r.rule.amount if isinstance(r.rule.amount, int) else 0”
- **What is weak:** `_prevented` honours a CANT_RECEIVE_DAMAGE threshold ("damage of N or less", GD05-123, Q416) only when `RuleMod.amount` is a literal int. `amount` is typed `Value` (`dsl.py:634`), so it could be a computed value, and any non-int becomes limit 0, which means "can't receive any damage". A scan of every card script in the registry this session found 125 RuleMods, 25 of them CANT_RECEIVE_DAMAGE (on 24 card definitions), and none with a non-integer amount. So the fallback is dead with today's pool, but a future card with a computed threshold would compile to total immunity with no error. W-55 lists other preconditions that hold only for today's pool, but not this one.
- **Resolution plan:** Rust keeps the behaviour for every literal amount. The registry build rejects a CANT_RECEIVE_DAMAGE RuleMod whose amount is not a literal integer, with an error naming the card (W-69 makes every such build error fail fast), so the card fails loudly instead of becoming immune. Evaluating a computed threshold would be a rules change, so it goes to the Python workflow if a card ever needs it. A data tripwire lists every RuleMod whose amount is not a literal, and fails when the list is not empty.
- **Verification** (unit + negative control): the tripwire passes on the packaged pool (0 non-literal amounts). As a negative control, a synthetic card in a temporary data copy whose threshold is Count-based fails the tripwire, and its registry build names the card. GD05-123's literal threshold behaves as in Python (the ported Q416 test).
- **Status:** open

### W-159 — Turn-history conditions match their filters against a card's current state, not its state when the event happened
- **Category:** engine · **Resolution type:** port-preserve + data tripwire
- **Sources:** `src/gcg_sim/engine/view.py:914-930`, `src/gcg_sim/engine/state.py:415-426`, `src/gcg_sim/engine/observe.py:86-95`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:212-216`, `src/gcg_sim/effects/bindings/wp_gd05_b.py:646-650`, `src/gcg_sim/effects/bindings/wp_gd03_a.py:555-559`, `src/gcg_sim/effects/bindings/wp_gd03_a.py:695-700`, `src/gcg_sim/effects/bindings/wp_gd04_a.py:610-621`, `docs/port/inventory/06b-engine-game-battle-observe.md:1018-1020`
- **Quotes:** “if c.filters and (h.uid < 0 or not matches(st, dv, ctx, h.uid, c.filters)):”
- **What is weak:** `HappenedThisTurn` counts this turn's history events. It checks `filters` against the event's card (`h.uid`) and `source_filters` against its source, both in the current derived view. `HistoryEvent` stores the card's `def_id` at the time of the event, but no condition reads it. Only `observe._own_history` touches the field, to rewrite it for determinized copies, which is why that rewrite is harmless today. So a card whose characteristics changed after the event stops matching the filter, or starts to. Five bindings filter on characteristics: GD05-129 (a destroyed UNIT, by a Neo Zeon source), GD03-081 (a deployed Unit with a trait), GD04-069 (a cost paid for a Militia or Dianna Counter Unit), GD03-069 (paired to this card) and GD05-089 (a Special Move Command activated). Example: GD05-089 deployed "as an (AP3･HP3) Unit" and then destroyed is a Pilot card again in the trash, so GD05-129's UNIT filter would not count it. The same applies to a trait or keyword granted by an effect that has since ended. This is inferred from the code. It has not been reproduced or checked against an official ruling. W-57's "turn-history gaps" were missing events, which is a different issue.
- **Resolution plan:** Rust keeps the behaviour exactly. Changing it is a rules interpretation, so it goes to the Python conflict workflow as a curated conflict. A pinned testkit case records Python's result for GD05-089 deployed as a Unit, then destroyed by a Neo Zeon card's effect, with GD05-129 in play. If no current Neo Zeon effect can destroy a friendly Unit, the case uses a test-only binding, and the entry records that the pool cannot reach it. A data tripwire lists every `HappenedThisTurn` filter that reads a characteristic that can change after the event (card kind, traits, keywords, pairing, stats), and fails when the list changes, so each new use is reviewed.
- **Verification** (unit + negative control): the pinned case gives Python's result in Rust. The tripwire lists exactly the five bindings above on the packaged pool. As a negative control, a synthetic binding in a temporary copy with a trait filter on a "discard" history condition fails it.
- **Status:** open

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
- **Status:** open

### W-161 — ToDeck can offer duplicate ARRANGE options when a targeted Unit's Pilot is also a target
- **Category:** engine · **Resolution type:** port-preserve + data tripwire
- **Sources:** `src/gcg_sim/engine/interp.py:781-810`, `src/gcg_sim/engine/interp.py:813-830`, `docs/port/inventory/06a-engine-core-interp.md:1186-1188`
- **Quotes:** “moved = [u for u in (*targets, *pilots) if st.cards[u].zone is Zone.DECK]”
- **What is weak:** `_h_to_deck` adds the Pilots of targeted battle-area Units to `moved` without removing duplicates. When a Pilot is also a target itself, `moved` and `together` hold it twice. `_todeck_next` then offers duplicate SELECT options in the owner's ARRANGE decision. Once the other cards are placed, it can ask one more ARRANGE decision whose two options are the same card. The cards end up in the same places, but the decision sequence changes: replay action lists, decision counts and AI branching. Whether the current pool can reach this is not established (inventory 06a, open question 1), and no other entry covers it.
- **Resolution plan:** Rust keeps the behaviour exactly, duplicates included, so replays and digests stay equal. A data tripwire scans the IR for ToDeck steps whose selector can choose a paired Pilot together with its Unit, and lists them. If the list is not empty, a testkit regression pins the duplicate options and the extra decision, and the case goes to the lead as a possible Python bug, to be fixed in both implementations together.
- **Verification** (unit): the tripwire runs on the packaged pool and its result is recorded. A testkit position with a test-only ToDeck that targets a Unit and its Pilot gives the same ARRANGE options (duplicates included) and the same number of decisions in Rust as in Python.
- **Status:** open

### W-162 — Trigger and frame fields that are serialised but never read (host_seq, optional, answer, Battle.blocked) may hide last-known-information bugs
- **Category:** engine · **Resolution type:** port-preserve + fuzz diagnostic
- **Sources:** `src/gcg_sim/engine/interp.py:61-77`, `src/gcg_sim/engine/core.py:248-283`, `src/gcg_sim/engine/core.py:354`, `src/gcg_sim/engine/game.py:712-727`, `src/gcg_sim/engine/state.py:145-156`, `src/gcg_sim/engine/state.py:266-278`, `src/gcg_sim/engine/state.py:326-339`, `src/gcg_sim/runner/digest.py:36-46`, `src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md:313`, `docs/port/inventory/06b-engine-game-battle-observe.md:1014-1017`, `docs/port/inventory/06a-engine-core-interp.md:1200-1201`
- **Quotes:** “host_seq = st.cards[host].zone_seq if host >= 0 else 0”
- **What is weak:** for a trigger of a card that is leaving play, `queue_trigger` receives the host's last-known `zone_seq` (`core.py:354`) and stores it in `TriggerInst.host_seq`. It uses the value for the 【Once per Turn】 key (`core.py:270`), but nothing else reads the field. `_resolve_trigger` does not pass it on, and `push_frame` recomputes `Frame.host_seq` from the host's current `zone_seq` (`interp.py:72`). A grep this session found no reader of `TriggerInst.host_seq` or `Frame.host_seq` outside serialisation. So a trigger whose host moved again between queueing and resolution resolves against the card in its new location, although rule 4-1-5 treats it as a new card there. Whether any current card reaches this is not established; it is plausible from the code. `Battle.blocked`, `damage_only` and `attack_triggers`, `TriggerInst.optional`, `Frame.answer` and `vars["__play"]` are also written, and hashed into the final digest through `GameState.to_json` (`digest.py:36-46`), but never read. W-54 covers only the PAYMENT decision kind and the ATTACK_TRIGGERS step.
- **Resolution plan:** Rust keeps every field, and its serialised position, for digest identity, and documents the unread ones as "serialised for digest identity, not read". In paranoid mode (W-49), the Rust engine records each host's `zone_seq` at the moment its trigger is queued, in a side table outside the game state, so digests are unchanged. It compares that value with the host's `zone_seq` when the trigger resolves, and counts and lists the cards where they differ. (Comparing with the queued `TriggerInst.host_seq`, as first proposed, would flag every 【Destroyed】 trigger, because that field holds the pre-move `zone_seq`.) Any card the fuzz finds goes to the Python conflict workflow as a rules question: should the effect use the card in its new location, last-known information, or do nothing? Only its resolution may change the behaviour, in both implementations together.
- **Verification** (unit + fuzz): unit: a testkit position where a host moves again before its trigger resolves increments the counter. One example is a 【Destroyed】 trigger whose card an earlier trigger of the same batch returns from the trash to the hand; a test-only binding may build it if no card reaches it. An ordinary 【Destroyed】 trigger does not increment the counter. Fuzz: 10,000 games in paranoid mode report the counter and list every card that triggered it, and final digests equal Python's.
- **Status:** open

### W-163 — The Burst acceptance score assumes that no Burst should be declined, which was checked only on the example and test decks
- **Category:** ai-policy · **Resolution type:** AI (strengthened search) + data tripwire
- **Sources:** `docs/AI.md:161`, `src/gcg_sim/ai/config.py:67`, `src/gcg_sim/ai/policy.py:309-310`
- **Quotes:** “no Burst in the example or test decks should be declined”
- **What is weak:** with `policy_burst_yes_score = 4.0` in the presets, a rollout at temperature 0.5 accepts a Burst 99.97% of the time (it declines 0.03%). The stated justification covers only the example and test decks. A scan this session of the 252 【Burst】 lines in the packaged pool (one per card) found none with an obvious drawback: the lines that mention the trash or "instead" are benefits, such as returning a card from the trash to the hand. So the assumption holds today. Nothing checks it for future cards, though: a Burst with a cost, a step that harms its controller's own cards, or an "instead" choice would be accepted in almost every rollout. The root decision is searched, but rollouts and the modelled opponent use the policy. W-27 treats Bursts as fixed and plans only for generic YES_NO decisions.
- **Resolution plan:** the reference keeps the score. A data tripwire lists every Burst whose IR contains a cost, a step whose chosen or affected cards include its controller's own (other than adding cards to the hand or deploying this card), or an optional replacement. It fails when the list changes, so each new one is reviewed. The strengthened policy scores the Bursts on that list with W-24's one-ply look-ahead, as W-27 does for YES_NO, and keeps the high score for the others.
- **Verification** (unit + negative control): the tripwire's list on the packaged pool is recorded and reviewed. It is expected to hold GD05-089, whose "you may deploy it … instead" is an optional replacement, and no harmful Burst. As a negative control, a synthetic Burst "Discard 2 cards. Draw 1." in a temporary data copy fails the tripwire. Unit tests on rollout distributions: the strengthened policy declines that synthetic Burst at least 90% of the time in a position where the look-ahead shows a loss, and accepts the packaged Bursts at least 99.9% of the time.
- **Status:** open

### W-164 — Token-definition fallbacks and two override-loader forms are never exercised by current data
- **Category:** data · **Resolution type:** Rust fix (implemented with synthetic-data tests) + data tripwire
- **Sources:** `src/gcg_sim/cards/db.py:80-97`, `src/gcg_sim/cards/db.py:131-138`, `src/gcg_sim/cards/db.py:199-241`, `docs/port/inventory/03-data-deck.md:935-937`
- **Quotes:** “card_number=f"TOKEN:{s.name}:{s.ap}/{s.hp}",”; “Synthetic `TOKEN:` defs never occur with the current data, and the `field_overrides`”
- **What is weak:** an inline token definition resolves to the lowest-numbered token card with the same name, AP, HP and traits. If none matches, a synthetic `TOKEN:name:ap/hp` card is created. A check this session found that no two of the 29 token cards share name, stats and traits, and that no definition needs a synthetic card, so both the tie-break and the synthetic path are dead today. Two override forms are unused too (inventory 03, open question 4): the nested per-number `field_overrides` form (`db.py:136-137`) and the singular `card_number` resolution key (`db.py:90-92`). A refresh that triggers one of these paths would take an untested route. The third fallback, which infers the card number from the conflict id (`db.py:93-94`), is live, contrary to the review that proposed this entry. Fifty resolutions that change card data, such as the canonical-printing choice `divergent:EXB-001`, have no card-number field and rely on it (counted this session). W-54 covers dead enum values only.
- **Resolution plan:** Rust implements all of these paths exactly. Each dead path gets a synthetic-data unit test, with expected values exported from Python: a token definition with no matching card, two matching token cards, a nested per-number override, and a singular `card_number` key. The live id inference is covered by the packaged data. A data tripwire fails when the packaged data first triggers a dead path, so the change is reviewed.
- **Verification** (unit + negative control): the synthetic tests give Python's card definitions (number, name, stats and `def_id` order). The tripwire passes on the packaged data, and each synthetic copy above makes it fail, naming the path. For all 50 id-inferred resolutions, the packaged data resolves the same card data as Python.
- **Status:** open
