#!/usr/bin/env bash
# The distributable gcg-sim binary runs fully offline and without Python (docs/dev/PLAN.md
# §9.11, T-145; the Rust counterpart of scripts/verify_wheel_offline.sh). The binary and the
# example decks are copied into a fresh directory outside the repository, then, with networking
# denied by sandbox-exec and an environment holding only HOME and an empty PATH (so no python,
# python3 or uv can be found; the binary calls /usr/sbin/sysctl by absolute path, PLAN A4), it
# runs data status, validate, the benchmark of every example pairing (BO1, --ai-preset fast, 2
# matches) at --workers 1, 4 and 10 (each pairing's results.json, games.ndjson and replays must
# be byte-identical across worker counts; W-118) and replay of every saved replay. Any failure
# fails the script.
#
#   RUSTFLAGS="-C target-cpu=apple-m2" cargo build --profile dist --locked --offline -p gcg-cli
#   scripts/verify_binary_offline.sh target/dist/gcg-sim
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: scripts/verify_binary_offline.sh BINARY" >&2
  exit 2
fi
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
if [[ ! -x "$binary" ]]; then
  echo "error: $1 is not an executable file" >&2
  exit 2
fi
if ! command -v sandbox-exec >/dev/null 2>&1; then
  echo "error: sandbox-exec is needed to deny networking (macOS)" >&2
  exit 2
fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
case "$work" in
  "$repo"/*) echo "error: the temporary directory is inside the repository" >&2; exit 2 ;;
esac

# In a subshell, so a `set -u` error fails the script under macOS /bin/bash 3.2, which exits 0
# from it when an EXIT trap is set (as in scripts/verify_all_rust.sh).
(
mkdir "$work/empty" "$work/bin"
cp "$binary" "$work/bin/gcg-sim"
cp "$repo"/examples/decks/*.txt "$work/"
cd "$work"

offline() {
  /usr/bin/env -i HOME="$work" PATH="$work/empty" \
    /usr/bin/sandbox-exec -p '(version 1)(allow default)(deny network*)' "$@"
}

gcg="$work/bin/gcg-sim"
offline "$gcg" data status
offline "$gcg" validate red-green-zeon.txt
offline "$gcg" validate blue-white-federation.txt
offline "$gcg" validate purple-white-tekkadan.txt
pairings=(
  "red-green-zeon.txt blue-white-federation.txt"
  "blue-white-federation.txt purple-white-tekkadan.txt"
  "purple-white-tekkadan.txt red-green-zeon.txt"
)
for i in "${!pairings[@]}"; do
  read -r dut bench <<<"${pairings[$i]}"
  for workers in 1 4 10; do
    offline "$gcg" benchmark "$dut" "$bench" \
      --matches 2 --seed 11 --format bo1 --ai-preset fast --workers "$workers" \
      --out "run-p$i-w$workers"
  done
  for workers in 4 10; do
    cmp "run-p$i-w1/results.json" "run-p$i-w$workers/results.json"
    cmp "run-p$i-w1/games.ndjson" "run-p$i-w$workers/games.ndjson"
    diff -r "run-p$i-w1/replays" "run-p$i-w$workers/replays"
  done
  for replay in "run-p$i-w1"/replays/*.json; do
    offline "$gcg" replay "$replay"
  done
done
echo "offline binary check passed: $(basename "$binary") (${#pairings[@]} pairings, workers 1/4/10)"
)
