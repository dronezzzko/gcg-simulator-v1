#!/usr/bin/env bash
# Acceptance criteria 1 and 2 for the built wheel: install it into a fresh venv outside the
# repository, then, with networking disabled, validate a deck, benchmark the example decks with
# 1 and 2 workers (results.json must be byte-identical) and verify a saved replay.
#
#   scripts/verify_wheel_offline.sh [dist/gcg_sim-<version>-py3-none-any.whl]
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wheel="${1:-$(ls "$repo"/dist/gcg_sim-*.whl | tail -n 1)}"
wheel="$(cd "$(dirname "$wheel")" && pwd)/$(basename "$wheel")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

offline() {
  if command -v sandbox-exec >/dev/null 2>&1; then
    sandbox-exec -p '(version 1)(allow default)(deny network*)' "$@"
  elif unshare -rn true 2>/dev/null; then
    unshare -rn "$@"
  else
    echo "error: cannot disable networking (needs sandbox-exec or unshare -rn)" >&2
    return 2
  fi
}

uv venv --quiet --python 3.12 "$work/venv"
uv pip install --quiet --offline --python "$work/venv/bin/python" "$wheel"
cp "$repo"/examples/decks/*.txt "$work/"
cd "$work"
gcg="$work/venv/bin/gcg-sim"

offline "$gcg" data status
offline "$gcg" validate red-green-zeon.txt
for workers in 1 2; do
  offline "$gcg" benchmark red-green-zeon.txt blue-white-federation.txt \
    --matches 2 --seed 11 --workers "$workers" --out "run-w$workers"
done
cmp run-w1/results.json run-w2/results.json
offline "$gcg" replay "$(ls run-w1/replays/*.json | head -n 1)"
echo "offline wheel check passed: $(basename "$wheel")"
