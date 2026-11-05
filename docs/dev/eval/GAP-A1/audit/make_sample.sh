#!/bin/bash
# W-17 step (3): the deterministic sample of the larger blinded audit (docs/dev/WEAKNESSES.md
# W-17), 20 BO1 games per example pairing for each AI version, in the audit protocol of
# scripts/blind_transcripts.py (benchmark runs with every replay saved), then the deterministic
# part of the audit (`gcg-eval weakness audit`) and the blinded transcripts for the auditors.
#
# Versions: the shipped strengthened `standard` (`gcg-sim benchmark --ai-preset standard`) and
# the reference `standard` (`gcg-eval reference-benchmark --reference-preset standard`), each
# playing both seats. Masters (dev role, A14): 22710601, 22710602, 22710603 for the pairings in
# the order of the 2026-10 audit; the audit's world master 22710700.
#
# Usage, from the repository root with release binaries in target/release:
#   bash docs/dev/eval/GAP-A1/audit/make_sample.sh [runs|count|transcripts OUT_DIR KEY_FILE]
set -euo pipefail
cd "$(dirname "$0")/../../../../.."
RUNS=docs/dev/eval/GAP-A1/audit/runs
BIN=${BIN:-target/release}
PAIRINGS=(
  "red-green-zeon blue-white-federation"
  "blue-white-federation purple-white-tekkadan"
  "purple-white-tekkadan red-green-zeon"
)

runs() {
  local n=0
  for pair in "${PAIRINGS[@]}"; do
    set -- $pair
    n=$((n + 1))
    local seed=$((22710600 + n))
    "$BIN/gcg-sim" benchmark "examples/decks/$1.txt" "examples/decks/$2.txt" --format bo1 \
      --matches 20 --seed "$seed" --workers 4 --replays 20 --ai-preset standard \
      --out "$RUNS/search-standard/$1-vs-$2"
    "$BIN/gcg-eval" reference-benchmark "examples/decks/$1.txt" "examples/decks/$2.txt" \
      --format bo1 --matches 20 --seed "$seed" --workers 4 --replays 20 \
      --reference-preset standard --out "$RUNS/mcts-standard/$1-vs-$2"
  done
}

count() {
  local dirs
  dirs=$(ls -d "$RUNS"/*/* | tr '\n' ',' | sed 's/,$//')
  "$BIN/gcg-eval" weakness audit --runs "$dirs" --policy search:standard --worlds 4 \
    --seed 22710700 --role dev --ledger docs/dev/eval/GAP-A1/seed-ledger.json --workers 4 \
    --out docs/dev/eval/GAP-A1/weakness/audit-report.json
}

transcripts() {
  # The blinded transcripts (W-119 ports this script to Rust; the Python one is fixed and tested
  # and re-plays the Rust replays, whose outcomes Python reproduces).
  uv run python scripts/blind_transcripts.py "$1" "$2" "$RUNS"/*/*
}

case "${1:-runs}" in
  runs) runs ;;
  count) count ;;
  transcripts) transcripts "$2" "$3" ;;
  *) echo "usage: $0 [runs|count|transcripts OUT_DIR KEY_FILE]" >&2; exit 2 ;;
esac
