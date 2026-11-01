#!/usr/bin/env bash
# The distributable gcg-sim binary runs fully offline and without Python (docs/port/PLAN.md
# §9.11, T-145; the Rust counterpart of scripts/verify_wheel_offline.sh). The binary and the
# example decks are copied into a fresh directory outside the repository, then, with networking
# denied by sandbox-exec and an environment holding only HOME and an empty PATH (so no python,
# python3 or uv can be found; the binary calls /usr/sbin/sysctl by absolute path, PLAN A4), it
# runs data status, validate, the example benchmark at --workers 1 and 2 (results.json must be
# byte-identical) and replay of a saved replay. Any failure fails the script.
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
for workers in 1 2; do
  offline "$gcg" benchmark red-green-zeon.txt blue-white-federation.txt \
    --matches 2 --seed 11 --workers "$workers" --out "run-w$workers"
done
cmp run-w1/results.json run-w2/results.json
replay="$(ls run-w1/replays/*.json | head -n 1)"
offline "$gcg" replay "$replay"
echo "offline binary check passed: $(basename "$binary")"
