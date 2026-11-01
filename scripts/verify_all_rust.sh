#!/usr/bin/env bash
# Full Rust gate on a clean clone of the committed HEAD (uncommitted changes are not included).
# Mirrors scripts/verify_all.sh (docs/port/PLAN.md §9.11, D17).
#
#   scripts/verify_all_rust.sh          # every gate that runs offline from the committed tree
#   scripts/verify_all_rust.sh --full   # plus the Python-side regeneration checks (needs uv;
#                                       # tier 2 takes hours)
#
# The script holds the gates whose commands exist at the current milestone, so it passes on a
# clean clone at every milestone. PLAN §9.11 lists the final script; a workstream's lines join
# here in the milestone that lands their command (the engine replay, fuzz and paranoid runs
# from M5.5, the gcg-sim/gcg-eval runs and the offline binary check from M7-M8).
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
branch="$(git -C "$repo" rev-parse --abbrev-ref HEAD)"
full=0
case "${1:-}" in
  "") ;;
  --full) full=1 ;;
  *) echo "usage: scripts/verify_all_rust.sh [--full]" >&2; exit 2 ;;
esac
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

git clone --quiet --no-hardlinks --branch "$branch" "$repo" "$work/repo"
cd "$work/repo"
echo "verifying $(git rev-parse --short HEAD) on $branch in $work/repo"

# SLOW = every <pkg>/slow, DIAG = every <pkg>/paranoid and <pkg>/testkit, read from cargo
# metadata. The helper exits 1 if any [[test]] or [[bin]] target requires a feature outside
# slow, paranoid and testkit, so no test target can stay unbuilt.
eval "$(cargo metadata --format-version 1 --no-deps --offline | python3 scripts/cargo_test_features.py)"

cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --locked --offline --features "$SLOW,$DIAG" \
  --target-dir target/paranoid -- -D warnings
cargo build --release --locked --offline -p gcg-diff
cargo build --release --locked --offline -p gcg-diff --features paranoid --target-dir target/paranoid
cargo test --workspace --release --locked --offline
cargo test --workspace --release --locked --offline --features "$SLOW"
cargo test --release --locked --offline -p gcg-engine --features paranoid,testkit \
  --target-dir target/paranoid

target/release/gcg-diff lints
target/release/gcg-diff selftest
target/release/gcg-diff corpus --tier 1 --parse-only --require-all
target/release/gcg-diff corpus --tier 1 --mock --require-all
target/release/gcg-diff tests-json --check
target/release/gcg-diff weaknesses --check docs/port/WEAKNESSES.md

if cargo tree -p gcg-cli -e normal --offline | grep -q gcg-ai-reference; then
  echo "gcg-cli depends on the reference search" >&2; exit 1; fi
if cargo tree -p gcg-cli -e features --offline | grep -q paranoid; then
  echo "gcg-sim would be built with the paranoid feature" >&2; exit 1; fi

# The Python reference stays untouched (PLAN A15).
git diff --quiet d9d41af -- src/gcg_sim tests
if [[ -n "$(git status --porcelain --untracked-files=all -- src/gcg_sim tests)" ]]; then
  echo "files were added under src/gcg_sim or tests" >&2; exit 1; fi

if [[ $full == 1 ]]; then
  uv sync --locked
  uv run ruff check port && uv run ruff format --check port
  MYPYPATH=src uv run mypy --strict port/oracle
  PYTHONHASHSEED=1 uv run python port/oracle/traces.py --check --tier 1
  PYTHONHASHSEED=2 uv run python port/oracle/traces.py --check --tier 1
  uv run python port/oracle/traces.py --check --tier 1 --nocache
  uv run python port/oracle/traces.py vectors --check
  uv run python port/oracle/census.py --check
  uv run python port/oracle/traces.py --write --tier 2
  target/release/gcg-diff corpus --tier 2 --parse-only --require-all
fi

if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
  echo "the gate wrote files into the tree:" >&2; git status --short >&2; exit 1; fi
echo "verify_all_rust passed"
