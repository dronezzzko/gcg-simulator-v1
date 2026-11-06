# WEAKNESSES.md: items not fully resolved

This file lists every item of `docs/dev/WEAKNESSES.md` whose status is `open`, as of 2026-11-04 at HEAD `a926ac7`. The crates and evaluation records at that commit are those of `d20f04b`: `a926ac7` adds the dist build only. For each item it gives:

- what is missing;
- why it is missing;
- the work that would close it;
- whether it is an AI item that needs ablation rows (PLAN §12.4);
- an effort estimate.

The statuses come from the package records in `docs/dev/eval/weakness-status/` and were checked against the repository. The first version of this file (2026-11-02, HEAD `d750b6c`) listed 33 open items. The gap-closing packages GAP-A1, GAP-A2, GAP-B, GAP-C and GAP-D (commit b8483dc) closed or measured most of them, and M15b (`docs/dev/eval/M15b/README.md`, commit 8c89b5e) adopted two of their mechanisms into both presets. Each entry's "Status" line in WEAKNESSES.md gives the same remaining work in short. This file also lists:

- resolved items whose slow evidence ran on the presets from before M15b;
- resolved items that carry a follow-up or a hand-off;
- bookkeeping for `docs/dev/tests.json`.

The first final evaluation (E1-E4 on the M15 presets, eval masters 90200001-90200006) was not accepted (`docs/dev/eval/final-1/README.md`). The second runs on eval masters 90300001-90300006 with the M15b presets, into `docs/dev/eval/final-2/`, and was still running when this file was written.

Effort: **S** is under a day, or one command plus its record. **M** is one to three days, or several hours of compute. **L** is more than three days.

AI row: "yes" means the work changes a strengthened-search mechanism or adds one. Such work needs an individual row, and a leave-one-out row from the shipped bundle if the mechanism is adopted (§12.4, §12.10). "no" means a non-AI item. It needs the A13 "no AI effect" evidence: the `search` and `baselines` corpus sets at 100%, plus the seed-1 equivalence smoke, wherever it touches code the AI executes.

## Counts

`target/release/gcg-diff weaknesses --check docs/dev/WEAKNESSES.md` passes. With `--require-resolved` it reports 2 problems, one per open item:

| Status | Items | Count |
| --- | --- | ---: |
| resolved | — | 129 |
| resolved by measurement (not adopted) | W-02, W-06, W-07, W-08, W-09, W-10, W-11, W-12, W-14, W-16, W-23, W-24, W-27, W-28, W-29, W-30, W-31, W-34, W-35, W-36, W-37, W-38, W-45, W-134, W-138, W-139, W-140, W-141, W-142, W-05 | 30 |
| admissible exception | W-131, W-132, W-133 | 3 |
| open: measured, verification not met | W-18, W-19 | 2 |

Changes since the first version of this file:

- resolved: W-13, W-25, W-26, W-64, W-75, W-76, W-88, W-104, W-110, W-118, W-119 and W-145 (the gap-closing records and M15b), and W-129 (Phase 8 documentation, commit 8c89b5e);
- resolved by measurement (not adopted): W-08, W-14, W-31, W-36, W-134, W-139 and W-142;
- by the lead's decisions of `docs/dev/PLAN.md` §0.1 (end of Phase 6): W-05 resolved by measurement (the break-even share replaces the scenario bar), W-63 and W-67 resolved (`templates/check.py --check` is T-154's assertion; no IR `template` field) and W-111 resolved (tag-based coverage stands for rules, FAQ entries and rulings). W-137's adoption question is decided (not adopted); its Linux run remains;
- resolved: W-101 (engine anchors for the last rule-level interpretations I-04, I-11, I-13 and I-17; I-21 and I-24 are compile-time readings with no engine path; section 3).

The 2 open items are AI items (class S): W-18 and W-19.

## 1. Lead: the final evaluation (docs/dev/eval/final-2/, docs/dev/eval/review/)

W-04 is resolved by final evaluation 2 and the reviewer's review-seed run: `standard` beats reference `strong` with lower bounds above 0.5 (0.563 and 0.562).

### W-19: the acceptance gates

- **Missing:** E4 (`standard` against `greedy`, lower bound above 0.764) on the AI test decks: 0.7237 (final 1), 0.7158 (final 2, 304/400), 0.7184 (review, 305/400); on the review seeds also over the held-out panel pairs alone (0.7623). E1's held-out pooled bound (A26) is 0.548 (final 2) and 0.553 (review).
- **Why:** on the two test decks the shipped `standard` beats `greedy` about 4 points more often than the reference (about 305 against 289 of 400); a lower bound above 0.764 needs about 323/400.
- **Work:** a stronger `standard` against `greedy`, tuned on tuning seeds only, then a third final evaluation and a review run on fresh seeds. A user decision (cost: days of machine time).
- **AI row:** yes. **Effort:** L.

## 2. gcg-search: mechanisms and AI evidence

### W-05: strategy fusion (the scenario bar) — decided, resolved by measurement

- **Decision (lead, `docs/dev/PLAN.md` §0.1):** the measured break-even share replaces the bar. The entry below is kept as the record of what was missing.
- **Missing:** the scenario bar: strengthened avoids the attack in at least 13 of 16 seeds. Measured: 16/16 attacks for the reference and `opponent_buckets=4` and `=8`.
- **Why:** under `ROLE_WEIGHTS` the two moves tie at a trick share of 0.497 (`gapa1_w05_fusion_values.rs`), so at a share of one half they differ by 0.0002 in the search's own evaluation. No setting of the search makes 13 of 16 a reliable result. The census half is done: every observation-key split has a key that removes it (`transpositions`, measured null), so 0 splits have neither (`gcg-eval weakness census`, GAP-A1).
- **Work:** the lead records whether the measured break-even share replaces the bar. Otherwise the item waits for a leaf evaluation that prices the bounce; the W-34 and W-35 refits measured null or negative.
- **AI row:** no for the decision; yes for a new evaluation. **Effort:** S as a decision.

### W-17: the blinded audit — resolved

- **Resolution (2026-11-04):** the audit ran on the shipped M15b presets with five auditor agents that did not generate the games (`docs/dev/eval/final-2/audit/README.md`): confirmed implausible plays per fully read game 1.33 (shipped `standard`) against 2.50 (reference), games with a confirmed missed lethal 0/60 against 2/60. Full reads cover a stratified 12 of the 120 games (a recorded deviation); every rule-flagged turn is checked.

### W-18: fidelity on the shipped M15b presets

- **Measured (2026-11-04):** `gcg-eval fidelity search:fast search:standard` with the five variants of the command below, eval master 90300007 (run once; ledger `docs/dev/eval/final-2/seed-ledger-fidelity.json`), report `docs/dev/eval/final-2/fidelity-fast-standard.json`. `w18_fidelity_check.py`: Spearman 0.9, Kendall tau-b 0.8, pairwise agreement 0.9, same best variant; `fast` favours the same deck wherever `standard` resolves a matchup; the paired difference of `blue-white-federation-weakened` is -0.075 [-0.146, -0.003] ("Verification: NOT met"). The other four intervals include 0.
- **Missing:** a `fast` preset whose paired differences all pass on held-out seeds.
- **Why:** on that variant `fast` (59/200) wins less than `standard` (74/200) on the same deals. On the M15 presets the same protocol met the verification on a confirm seed.
- **Work:** tune `fast` (for example a larger budget, docs/dev/eval/M15 budget curves) on tuning seeds against this protocol, then rerun E2 and the fidelity check on fresh eval seeds. This changes the evaluated preset, so it is a new strength round.
- **AI row:** yes (a preset change). **Effort:** M-L (compute: the E2 rerun).

  The command of the run:

  ```text
  gcg-eval fidelity search:fast search:standard --variants red-green-zeon,tests/data/decks/red-green-zeon-weakened.txt,purple-white-tekkadan,docs/dev/eval/P6-D/decks/purple-white-tekkadan-weakened.txt,docs/dev/eval/P6-D/decks/blue-white-federation-weakened.txt --against blue-white-federation --games 200 --seed 90300007 --role eval --ledger docs/dev/eval/final-2/seed-ledger-fidelity.json --workers 3 --out docs/dev/eval/final-2/fidelity-fast-standard.json
  uv run python docs/port/eval/GAP-A2/w18_fidelity_check.py docs/dev/eval/final-2/fidelity-fast-standard.json
  ```

### W-137: determinism contract (strengthened-search math) — resolved

- **Resolution (2026-11-05):** `portable_math` is not adopted (lead decision), and the Linux x86-64 run passed: GitHub Actions run 42070860469 (PR #6), job `portable-math-linux`, `gapa2_portable_math` 3 passed.

## 3. gcg-engine: interface changes

### W-101: interpretation-tagged code paths — resolved

- **Resolution:** the engine anchors the last four rule-level readings, each tested in the scenario of its Python test with a control that does not record it (`crates/gcg-engine/tests/gapb_interpretations.rs`):
  - I-04 at an `Apply` on an `All` selection (`crates/gcg-engine/src/interp/lasting.rs:76-78`);
  - I-11 at the playability walk of a mode selection (`crates/gcg-engine/src/interp/choose.rs:79`) and at the mode choice (`crates/gcg-engine/src/interp/mod.rs:696`);
  - I-13 where a card becomes its Unit variant (`crates/gcg-engine/src/interp/deploy.rs:139`) and where a variant leaves the battle area (`crates/gcg-engine/src/core.rs:220-226`);
  - I-17 where a linked pairing emits LINKED (`crates/gcg-engine/src/interp/play.rs:92-93`).

  I-21 has no engine code path: the compiler puts a "Then" part inside its leading "If"/"If you do" block (`src/gcg_sim/effects/compiler/grammar.py:1641-1645`, `:1660-1680`), and the engine runs that block's `JumpIfNot` or `JumpIfNotDid` as it runs every other (`crates/gcg-engine/src/interp/mod.rs:602-621`). I-21 and I-24 are the two interpretations in `not_traceable_from_games`. 14 of the 27 are anchored, 12 of the 14 rule-level ones. The tier-1 corpus stays at 3804/3804; the tier-2 corpus is the lead's run after the merge. The entry below is kept as the record of what was missing.
- **Missing:** engine anchors for the rule-level interpretations I-04 (lasting "all your Units"), I-11 (modal Commands), I-13 (deploy as an APx･HPy Unit), I-17 (【When Linked】 on pairing) and I-21 ("If"/"Then"). I-24 is a compile-time reading with no engine path. The card-level interpretations keep the card trace, which over-approximates.
- **Why:** GAP-B anchored 10 of the 27 (I-01, I-02, I-03, I-05, I-06, I-10, I-14, I-15, I-16, I-18; 8 of the 14 rule-level ones) through `gcg_engine::diag::Interpretation`. The others need their code paths identified in `crates/gcg-engine/src/`.
- **Work:**
  - Add a `note_interpretation` call at each remaining anchored code path, read-only and absent from every compared output, like the existing ten.
  - Extend `crates/gcg-engine/tests/gapb_interpretations.rs` and `crates/gcg-runner/tests/p6m_analysis.rs` with a game that runs each new anchor.
  - The tier-1 and tier-2 corpus must stay at 100%.
- **AI row:** no (A13: a counter that the AI does not read). **Effort:** M.

### W-111: execution coverage of rules, FAQ entries and rulings — decided, resolved

- **Decision (lead, `docs/dev/PLAN.md` §0.1):** tag-based coverage stands (the first option below).
- **Missing:** the lead's decision for rules, FAQ entries and rulings. They stay tag-based, while the verification's bar is 100% execution evidence for every one. Cards have execution evidence.
- **Why:** execution of a rule's code path needs per-rule code-path ids: about 395 testable rules over about 14,600 lines of `gcg-engine/src` (GAP-B).
- **Work:** one of:
  - record that tag-based coverage stands for rules, FAQ entries and rulings, with the reason. GAP-B recommends this: every tagged Python test is replayed decision by decision against Python's trace (3213/3213 per-test traces, 311/311 census games), so a passing tagged test reproduces Python's behaviour exactly;
  - or extend the W-101 interpretation counters rule by rule and read them in `crates/gcg-diff/src/coverage.rs` while the per-test traces replay.
- **AI row:** no. **Effort:** S for a decision, L for the probes.

## 4. gcg-eval and gcg-data: opt-in tools (A30)

### W-63 and W-67: the IR `template` field and T-154 — decided, resolved

- **Decision (lead, `docs/dev/PLAN.md` §0.1):** the second option below: `templates/check.py --check` and the checked report stand for both; no IR `template` field.
- **Missing:**
  - The IR `cards[].template` field, written by `oracle/export_ir.py`.
  - T-154: `export_ir.py` asserts that every template reproduces its binding cards byte for byte.
- **Why:** setting the field changes the bytes of `crates/gcg-effects/ir/registry.json` and so the `generator.ir_sha256` of every corpus trace (`crates/gcg-diff/src/corpus.rs:175`). The whole tier-1 and tier-2 corpus would have to be regenerated, and GAP-C did not do that while the final evaluation ran. Everything else is done (GAP-C): 20 general templates cover every listed family, 24 of the 302 binding cards are reproduced byte for byte, 6 are reviewed differences, and `gcg-eval data-status --templates` reports the counts from the checked `docs/dev/audit/templates.json`.
- **Work:** one of:
  - set `cards[].template` in `export_ir.py` (the IR slot and the loader `CardEntry::template` exist), run `uv run python oracle/export_ir.py --write`, regenerate the corpus (`oracle/pytest_trace.py --write`, `oracle/traces.py --write --tier 1`, then tier 2), and add a call to `templates.check.report()` (mismatches empty) in `export_ir.py`'s check; golden, corpus and equivalence runs must show no behaviour change;
  - or record that the checked report and `oracle/templates/check.py --check`, which runs in `scripts/verify_all_rust.sh --full`, stand for both.
- **Post-acceptance (answer (a)):** move the templates into `src/gcg_sim/effects/compiler/`, then retire the 24 reproduced bindings and the 32 that the plain compiler already reproduces (report key `compiler_reproduces_bindings`), with an empty golden diff.
- **AI row:** no. **Effort:** S as a record, M with the IR field.

## 5. gcg-runner

No item is open here. W-104 is resolved by the decision recorded in `docs/dev/eval/weakness-status/GAP-C.md`: `timing.json` keeps the deterministic `decisions` and `decisions_per_game` and adds no CPU-time or start-up field, because safe std has no CPU-time clock and a busy-seconds measure would be wall time again. The lead may override it (section 9).

## 6. Lead: gates, docs and long runs

### W-126: throughput reproducibility — resolved

- **Resolution:** the Phase 7 reviewer's idle-machine pair at 2d425db agrees within 0.33% (CPU s/game) and 0.27% (wall) on every pairing, `--agree` exit 0 (`docs/port/REVIEW.md` §6.2). The command takes an integer `--workers` (6 here).

### W-154: the Python gate with the port committed — resolved

- **Resolution:** the Phase 7 reviewer's clean-clone run of `scripts/verify_all.sh` at 7b7ab2f passed (3985 Python tests; `docs/port/REVIEW.md` §1, C2).

## 7. The user's decision — resolved

### W-120: project Claude permissions — resolved

- **Resolution (2026-11-05):** the user approved pinning curl to the two data hosts and dropping the malformed `git -C` rule; `scripts/check_claude_settings.py` reports 0 findings.

## 8. Resolved items to re-verify after the M15b preset change

The presets changed again at M15b (commit 8c89b5e: `kill_aware_targeting` and `tree_lethal_select` in both). The M15b gate ran `cargo test --workspace --release` (1320 passed), the slow `gcg-search` tests (306 passed) and the tier-1 corpus (`docs/port/progress.md`). These resolved items' slow or long evidence ran on the M15 presets, and no later run is recorded:

- **W-150:** the 1/4/10-worker identity of the shipped presets (`phase6_slow::w150_every_preset_is_identical_at_1_4_and_10_workers`; passed with the M15 presets in GAP-D's gate at 54c2bd2).
- **W-152:** seat symmetry with `search:fast` (`phase6_slow::w152_seats_and_first_player_are_symmetric`; M15 `search:fast` 198/400 at 54c2bd2).
- **W-110:** the 1,000 strategic paranoid fuzz games (`docs/dev/eval/GAP-D/w110-strategic-fuzz.txt`) used the M15 `search:fast`.

Re-verified on the M15b presets by the Phase 7 reviewer's `scripts/verify_all_rust.sh --full` at 7b7ab2f (`docs/port/REVIEW.md` §1, C1): the slow tier, `phase6_slow` included, passed, and the 2,000 commit-seed and 1,000 strategic paranoid fuzz games had 0 failures.

Already re-run on the M15b presets (2026-11-04, the crates of HEAD):

- W-25's over-extension scenario (`gapa1_w25_over_extension.rs`): shipped `fast` 0/16 and shipped `standard` 0/16 over-extensions, the reference 16/16.
- W-139's record (`gapb_tests::w139_held_trick_decisions_at_every_decision_are_recorded`): the shipped `standard` keeps the trick 16/16 in both positions; with `opponent_view_share=0.25` it keeps it 10/16 and 8/16 (15-16 on the M15 presets). The mechanism is not adopted.

## 9. Resolved items with follow-ups or hand-offs

None of these follow-ups blocks the item's resolution. They are listed so that none is lost.

| Item | Follow-up | Owner |
| --- | --- | --- |
| W-06 | `visited_set_backup` is a correctness fix that measured null (-0.005). The P6-D file leaves its adoption to the lead | lead (preset decision; leave-one-out row if adopted) |
| W-25, W-26 | §12.9 step 4 asks for a leave-one-out row of every adopted mechanism from the adopted bundle. The recorded rows are GAP-A1's leave-one-out rows from the M15 bundle and M15b's row of both flags against the M15 `standard` (+0.026 [-0.010, +0.062]); per-flag rows from the M15b bundle are not recorded | lead |
| W-25 | Plan items (2), the per-deck opponent profile, and (3), belief weighting by the opponent model, are not built, by GAP-A1's recorded decision, which asks for the lead's acceptance | lead |
| W-31, W-36, W-104, W-142 | Closed by a decision recorded in a package file (GAP-A2, GAP-C), not by a lead record; the lead may override | lead |
| W-36, W-142 | If a refit or `determinized_fit` is ever proposed for adoption, record leaf positions on the 13 held-out pairs under a fresh eval master after the final evaluation, and score the models with `--test-positions` first | lead |
| W-41 | Calibration against real tournament results once the user supplies data (A32 item 5) | user, then lead |
| W-46 | The head-to-head `unfinished` list is tested on recorded outcomes. The verification's e2e with a tiny engine decision cap is not separately recorded | lead or P6-I (S) |
| W-57 | The `tuning --rounds 0` bug row has no Rust test. P6-K says `gcg-eval tune` should carry it | P6-I (S) |
| W-63, W-67, W-68, W-129, W-155 | §12.11 post-acceptance package steps (answer (a)) | after acceptance, `/gcg-refresh-data` binding workflow |
| W-71, W-72 | The official ST02/ST05 decklists and the historical B&R lists through `/gcg-refresh-data` | lead |
| W-76 | Sealed support needs its rules page in the data | lead |
| W-89, W-97 | Repeat the `dataset_version` meaning and the schema naming rule in the Rust report docs | lead, Phase 8 |
| W-92 | The benchmark skill's sample-size guidance with the design effect | lead, Phase 8 (T-142) |
| W-114 | Mark the refresh skill's blocks and add `.claude/skills/gcg-refresh-data/**/*.md` to the doc checker's default set | lead |
| W-115, W-128 | Regenerate the research artefacts and the stale Python-doc counts (Python side) | lead |
| W-118 | The recorded clean-clone pass of `scripts/verify_all_rust.sh --full` is at 54c2bd2, a GAP-D branch commit merged by b8483dc. A run at the merged HEAD is T-001's acceptance step | lead, M16 |
| W-119 | `scripts/blind_transcripts.py:98` compares the card type with `"ex_resource"`; `CardType.EX_RESOURCE`'s value is `"EX RESOURCE"`, so every Python transcript prints `EX 0` (GAP-C) | lead |
| W-127 | The 16 dangling shas in `docs/PROGRESS.md` | lead (Python docs) |
| W-130 | `src/gcg_sim/data/official/floor_rules.json` wording, listed for the user (packaged data) | user |
| W-134 | The plan's CLI e2e with a one-list prior needs a prior field in `StrengthenedConfig` and a `gcg-eval benchmark --opponent-prior FILE` option; `MechanismEffort` counters for `view_kept_worlds` and the archetype candidates (GAP-B interface requests 1 and 2) | lead (`contracts.rs`) |
| W-139 | Residue: in the opponent-view worlds the opponent's nodes still see the named cards | lead, only if the mechanism is revisited |
| W-143, W-162 | Rules questions to the Python conflict workflow: redirect substitution once per damage event (W-143), and the 395 moved-host battle-damage triggers (W-162) | lead |
| W-145 | Make `gcg_data::deck::parse_deck_with` public and re-export it, so a refreshed-data tool can parse deck files over its own database (GAP-C) | lead |
| W-153 | The SKILL.md step 2 wording (replacement text in P6-N lead note 4) | lead, M17 (T-142) |
| W-155 | Run the parenthetical gate inside the refresh workflow | lead, M17 |

Done since the first version: W-15's sentence on the 250-decision cap and the continuation is in the Rust `docs/AI.md` (`docs/AI.md:172-176`), and W-45's reason for not adopting `paired_mulligan` is recorded (GAP-A2: its individual dev row is -0.060 [-0.137, +0.018]).

## 10. `docs/dev/tests.json` bookkeeping (lead file)

The register lags behind the package evidence. `gcg-diff tests-json --check` checks only its format and id list, and it passes.

- **Recorded as `not-started` or `failing`, but their tests pass according to the package files:**
  - scenario tests T-107, T-108, T-110, T-111, T-112, T-113, T-114, T-115, T-116, T-117, T-118, T-119, T-120, T-121 and T-122 (GAP-D's clean-clone gate at 54c2bd2, which lists each test; T-114, T-116 and T-118 also GAP-A1, T-113 also GAP-A2). T-118 was rerun on the M15b presets on 2026-11-04 (section 8);
  - T-109 and T-123 pass as records. W-05 stays open on its bar; W-139 is resolved by measurement, and GAP-B's record asserts its bar, which the shipped `standard` meets;
  - T-056, recorded as failing: the 2,000 commit-seed games and the 1,000 strategic paranoid games of W-110 passed (GAP-D);
  - T-001: the clean-clone `--full` run passed at 54c2bd2 (GAP-D), a branch commit before the merge;
  - T-130: `crates/gcg-search/tests/ai_md_presets.rs` passes (4 tests, run 2026-11-04);
  - T-145: the offline binary check passed in GAP-D's gate, and again on the dist binary (commit message of a926ac7);
  - T-153 (`gcg-eval benchmark` byte identity: GAP-D's gate line and `p6m_cli.rs::t153_*`);
  - T-155 (refresh news tests: `pytest oracle/tests` in GAP-D's `--full`, 25 passed).
- **Should stay open:**
  - T-124 to T-127: final evaluation 2.
  - T-131 (W-18): the fidelity run on the M15b presets.
  - T-133: the CI run after the user approves the push.
  - T-135 (W-154): `scripts/verify_all.sh` on a clean clone, M16.
  - T-154 (W-63, W-67): the `export_ir.py` assertion.
  - T-106 and T-007 need `gcg-diff weaknesses --check --require-resolved` to pass: 8 problems today.
- **P6-C's status file cites the wrong register ids.** It cites T-105 for W-05's unit test, T-107 for W-07's scenario and T-139 for W-139's unit test. The register's ids are T-109 (W-05), T-110 (W-07) and T-123 (W-139). WEAKNESSES.md uses the register's ids.
