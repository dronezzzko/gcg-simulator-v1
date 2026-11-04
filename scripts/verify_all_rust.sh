#!/usr/bin/env bash
# Full Rust gate on a clean clone of the committed HEAD (uncommitted changes are not included).
# Mirrors scripts/verify_all.sh (docs/port/PLAN.md §9.11, D17; W-118).
#
#   scripts/verify_all_rust.sh          # every gate that runs offline from the committed tree
#   scripts/verify_all_rust.sh --full   # plus the Python-side checks and regenerations (needs
#                                       # uv; the tier-2 corpus takes hours)
#
# Every step fails the gate (set -euo pipefail; scripts/verify_all_rust_selftest.sh injects a
# failing step and a failing pipeline into a copy and checks that the copy fails). Nothing is
# skipped: the slow tests that read the regenerated tier-2 corpus ($SLOW_TIER2) run in --full
# right after that corpus is written.
#
# GCG_GATE_WORKERS=N caps the workers of the corpus replays, the fuzz runs and the Python trace
# and census runs on a machine shared with other work (defaults: one corpus job per online CPU,
# `auto` fuzz workers, 8 Python workers). No output depends on it. The identity runs at 1, 4
# and 10 workers keep their counts.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
branch="$(git -C "$repo" rev-parse --abbrev-ref HEAD)"
full=0
case "${1:-}" in
  "") ;;
  --full) full=1 ;;
  *) echo "usage: scripts/verify_all_rust.sh [--full]" >&2; exit 2 ;;
esac
gate_workers="${GCG_GATE_WORKERS:-}"
if [[ -n "$gate_workers" && ! "$gate_workers" =~ ^[1-9][0-9]*$ ]]; then
  echo "GCG_GATE_WORKERS must be a positive integer, got '$gate_workers'" >&2; exit 2; fi
rust_workers="${gate_workers:-auto}"
py_workers="${gate_workers:-8}"
corpus_jobs="${gate_workers:-$(getconf _NPROCESSORS_ONLN)}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The steps run in a subshell: macOS /bin/bash 3.2 exits 0 from a `set -u` error when an EXIT
# trap is set, so the parent turns the subshell's failure into the gate's exit status (the
# unset-variable control of scripts/verify_all_rust_selftest.sh).
(
git clone --quiet --no-hardlinks --branch "$branch" "$repo" "$work/repo"
cd "$work/repo"
echo "verifying $(git rev-parse --short HEAD) on $branch in $work/repo"

# SLOW = every <pkg>/slow (SLOW_OFFLINE + SLOW_TIER2), DIAG = every <pkg>/paranoid and
# <pkg>/testkit, read from cargo metadata. The helper exits 1 if any [[test]] or [[bin]] target
# requires a feature outside slow, paranoid and testkit, so no test target can stay unbuilt.
eval "$(cargo metadata --format-version 1 --no-deps --offline | python3 scripts/cargo_test_features.py)"

# Build, lint and every offline test tier (W-112: the default tier, then the slow tier).
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --locked --offline --features "$SLOW,$DIAG" \
  --target-dir target/paranoid -- -D warnings
# gcg-diff builds on its own: it enables gcg-engine/testkit, which one build would unify into
# gcg-sim and gcg-eval (A31: they ship without it).
cargo build --release --locked --offline -p gcg-cli -p gcg-eval
cargo build --release --locked --offline -p gcg-diff
cargo build --release --locked --offline -p gcg-diff --features paranoid --target-dir target/paranoid
cargo test --workspace --release --locked --offline
cargo test --workspace --release --locked --offline --features "$SLOW_OFFLINE"
cargo test --release --locked --offline -p gcg-engine --features paranoid,testkit \
  --target-dir target/paranoid
# The opt-in zstd output (W-99) and the system allocator (--no-default-features), each in its
# own target directory so the release binaries above keep the shipped features.
cargo test --release --locked --offline -p gcg-eval --features zstd --target-dir target/zstd
cargo test --release --locked --offline -p gcg-cli -p gcg-eval --no-default-features \
  --target-dir target/sysalloc

# Differential corpus, negative controls, coverage, traceability and the registers.
target/release/gcg-diff lints
target/release/gcg-diff selftest
target/release/gcg-diff selftest --real-engine
target/release/gcg-diff corpus --tier 1 --parse-only --require-all --jobs "$corpus_jobs"
target/release/gcg-diff corpus --tier 1 --mock --require-all --jobs "$corpus_jobs"
target/release/gcg-diff corpus --tier 1 --mock --roundtrip --require-all --jobs "$corpus_jobs"
target/release/gcg-diff corpus --tier 1 --require-all --jobs "$corpus_jobs"
target/release/gcg-diff corpus --tier 1 --roundtrip --require-all --jobs "$corpus_jobs"
target/paranoid/release/gcg-diff corpus --tier 1 --require-all --paranoid --jobs "$corpus_jobs"
target/release/gcg-diff coverage --check
target/release/gcg-diff traceability --check
target/release/gcg-diff tests-json --check
target/release/gcg-diff weaknesses --check docs/port/WEAKNESSES.md
target/release/gcg-diff api-snapshot --check

# Fuzz with invariants after every action (PLAN §9.4): Python's seed-2026 games, a
# commit-derived seed, and the W-110 strategic games in the paranoid build.
target/release/gcg-diff fuzz --games 10000 --seed 2026 --workers "$rust_workers" \
  --compare port/corpus/v1/tier1/fuzz/summary-2026-10000.json
target/release/gcg-diff fuzz --games 2000 --seed-from-commit "$(git rev-parse HEAD)" \
  --workers "$rust_workers"
target/paranoid/release/gcg-diff paranoid-games
target/paranoid/release/gcg-diff fuzz --games 400 --seed 2026 --agent greedy --paranoid \
  --workers "$rust_workers"
target/paranoid/release/gcg-diff fuzz --games 400 --seed 2026 --agent search:fast --paranoid \
  --workers "$rust_workers"
target/paranoid/release/gcg-diff fuzz --games 200 --seed 2026 --agent mixed:greedy,search:fast \
  --paranoid --workers "$rust_workers"

# Evaluation registers: the held-out overlap bound and the evidence directory.
target/release/gcg-eval decks --overlap --check >/dev/null
target/release/gcg-eval evidence check

# gcg-sim never reaches the reference search or the paranoid build (PLAN §2.2).
if cargo tree -p gcg-cli -e normal --offline | grep -q gcg-ai-reference; then
  echo "gcg-cli depends on the reference search" >&2; exit 1; fi
if cargo tree -p gcg-cli -e features --offline | grep -q paranoid; then
  echo "gcg-sim would be built with the paranoid feature" >&2; exit 1; fi

# CLI runs: identical outputs at 1/4/10 workers for the shipped preset, the reference search
# and head-to-head; replays verify; the reports validate; --ai-preset takes only two values.
mkdir -p build
for d in examples/decks/*.txt examples/decks/eval/*.txt; do
  target/release/gcg-sim validate "$d" >/dev/null; done
for w in 1 4 10; do
  target/release/gcg-sim benchmark examples/decks/red-green-zeon.txt \
    examples/decks/blue-white-federation.txt --matches 4 --seed 7 --ai-preset fast \
    --workers "$w" --decision-log --out "build/w$w" >/dev/null
  target/release/gcg-eval reference-benchmark examples/decks/red-green-zeon.txt \
    examples/decks/blue-white-federation.txt --matches 4 --seed 7 --reference-preset fast \
    --workers "$w" --decision-log --out "build/ref-w$w" >/dev/null
  target/release/gcg-eval head-to-head search:fast ref:fast --decks examples --games 24 \
    --seed 20000001 --role dev --ledger "build/ledger-w$w.json" --workers "$w" \
    --out "build/h2h-w$w.json" >/dev/null
done
for kind in w ref-w; do
  python3 scripts/normalize_outputs.py "build/${kind}1" "build/${kind}1.norm"
  for w in 4 10; do
    python3 scripts/normalize_outputs.py "build/$kind$w" "build/$kind$w.norm"
    diff -r -x timing.json "build/${kind}1.norm" "build/$kind$w.norm"
  done
done
for w in 4 10; do cmp build/h2h-w1.json "build/h2h-w$w.json"; done
# A30 (T-153): gcg-eval benchmark without an opt-in option writes gcg-sim's bytes.
target/release/gcg-eval benchmark examples/decks/red-green-zeon.txt \
  examples/decks/blue-white-federation.txt --matches 4 --seed 7 --ai-preset fast \
  --workers 4 --decision-log --out build/eval-bench >/dev/null
diff -r -x timing.json build/w4 build/eval-bench
for f in build/w1/replays/*.json build/ref-w1/replays/*.json; do
  target/release/gcg-sim replay "$f" >/dev/null; done
target/release/gcg-diff schema-check build/w1/results.json build/ref-w1/results.json
target/release/gcg-diff schema-check crates/gcg-runner/tests/vectors/records/*/results.json
target/release/gcg-diff schema-check --expect-invalid crates/gcg-runner/tests/vectors/invalid-results/*.json
for p in strong STANDARD Fast bogus; do
  set +e
  target/release/gcg-sim benchmark examples/decks/red-green-zeon.txt \
    examples/decks/blue-white-federation.txt --matches 1 --ai-preset "$p" --out build/bad >/dev/null 2>&1
  rc=$?
  set -e
  [[ $rc == 2 ]] || { echo "--ai-preset $p: exit $rc, expected 2" >&2; exit 1; }
done

# The distributable binaries (T-143): the apple-m2 build replays the tier-1 corpus like the
# native one (T-092), and gcg-sim runs offline without Python (T-145).
scripts/build_dist.sh
target/dist/gcg-diff corpus --tier 1 --require-all --jobs "$corpus_jobs"
scripts/verify_binary_offline.sh target/dist/gcg-sim

# Stdlib-only tool checks, the gate's own negative controls, the CLI help snapshot, generated
# counts and the freshness summary (W-114, W-118, W-120's check, W-129, W-128, W-115, W-153,
# W-126's agreement check).
python3 scripts/check_doc_commands.py --selftest
python3 scripts/cli_help_snapshot.py --check
python3 scripts/throughput.py --selftest
scripts/verify_all_rust_selftest.sh
python3 scripts/check_claude_settings.py --selftest
python3 scripts/doc_counts.py --check
python3 scripts/gate_summary.py

# The Python reference stays untouched (PLAN A15).
git diff --quiet d9d41af -- src/gcg_sim tests
if [[ -n "$(git status --porcelain --untracked-files=all -- src/gcg_sim tests)" ]]; then
  echo "files were added under src/gcg_sim or tests" >&2; exit 1; fi

if [[ $full == 1 ]]; then
  uv sync --locked
  uv run ruff check . && uv run ruff format --check .
  MYPYPATH=src uv run mypy --strict port/oracle
  # The documented commands (W-114, W-154) run in their own clone of HEAD, so a regeneration
  # they document cannot change the tree under test. The only tracked file they may rewrite
  # there is docs/RULES_TRACEABILITY.md, which `gcg-diff traceability --write` generates
  # (checked above) and Python's generator would rewrite without the Rust column; the blocks
  # that do that, like the full Python suite, carry doc-check skip markers. Any other rewritten
  # file fails the gate.
  git clone --quiet --no-hardlinks "$work/repo" "$work/docs"
  (cd "$work/docs" && uv run python scripts/check_doc_commands.py --timeout 21600)
  rewritten="$(git -C "$work/docs" status --porcelain --untracked-files=all | cut -c4- |
    awk '$0 != "docs/RULES_TRACEABILITY.md"')"
  if [[ -n "$rewritten" ]]; then
    echo "the documented commands rewrote files of the tree:" >&2; echo "$rewritten" >&2; exit 1; fi
  if ! git -C "$work/docs" diff --quiet -- docs/RULES_TRACEABILITY.md; then
    echo "note: CLAUDE.md's Python traceability command rewrites docs/RULES_TRACEABILITY.md"; fi
  uv run python scripts/blind_transcripts.py --selftest
  uv run python scripts/fuzz_census.py --selftest
  uv run python scripts/fuzz_census.py --games 10000 --seed 2026 --workers "$py_workers" \
    --check docs/port/eval/P6-N/fuzz-census-2026-10000.json
  uv run python scripts/test_census.py --check docs/port/eval/P6-N/test-execution.json
  PYTHONHASHSEED=1 uv run python port/oracle/traces.py --check --tier 1 --workers "$py_workers"
  PYTHONHASHSEED=2 uv run python port/oracle/traces.py --check --tier 1 --workers "$py_workers"
  uv run python port/oracle/traces.py --check --tier 1 --nocache --workers "$py_workers"
  uv run python port/oracle/traces.py vectors --check
  uv run python port/oracle/census.py --check
  uv run python port/oracle/eval_decks.py --check
  uv run python port/oracle/p6k_vectors.py --check
  uv run python port/oracle/p6k_audit.py --check
  uv run python port/oracle/templates/check.py --check
  uv run pytest port/oracle/tests -q
  scripts/compare_python_vs_rust.sh equivalence --selftest
  scripts/compare_python_vs_rust.sh report --selftest
  COMPARE_OUT="build/comparison" scripts/compare_python_vs_rust.sh equivalence --matches 2 \
    --seeds 1 --formats bo1 --pairs red-green-zeon:blue-white-federation --workers 1 \
    --decision-logs off
  uv run python port/oracle/traces.py --write --tier 2 --workers "$py_workers"
  target/release/gcg-diff corpus --tier 2 --require-all --jobs "$corpus_jobs"
  cargo test --workspace --release --locked --offline --features "$SLOW_TIER2"
fi

if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
  echo "the gate wrote files into the tree:" >&2; git status --short >&2; exit 1; fi
echo "verify_all_rust passed"
)
