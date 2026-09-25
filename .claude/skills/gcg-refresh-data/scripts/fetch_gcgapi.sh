#!/usr/bin/env bash
# Fetch gcg-api card data (https://github.com/yzRobo/gcg-api) into a temporary git clone.
#
#   fetch_gcgapi.sh [--ref <commit|tag|branch>] [--dest DIR]
#
# Partial clone (--filter=blob:none) with a sparse checkout of data/, schema.sql and
# LICENSE-DATA only, so the site/worker code and any images are never downloaded. The clone
# root is what `python -m gcg_sim.tools.refresh diff|apply --new-data` expects; apply reads the
# commit from its HEAD. The last lines are shell assignments for the caller:
#   GCGAPI_DIR=...  GCGAPI_COMMIT=...  GCGAPI_RETRIEVED_AT=...
set -euo pipefail

repo_url="https://github.com/yzRobo/gcg-api"
ref=""
dest=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --ref) ref="${2:?--ref needs a value}"; shift 2 ;;
    --dest) dest="${2:?--dest needs a value}"; shift 2 ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$dest" ]]; then
  dest="$(mktemp -d "${TMPDIR:-/tmp}/gcg-api.XXXXXX")/gcg-api"
fi
if [[ -e "$dest" && -n "$(ls -A "$dest" 2>/dev/null)" ]]; then
  echo "destination $dest exists and is not empty" >&2
  exit 2
fi

retrieved_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
git clone --quiet --filter=blob:none --no-checkout "$repo_url" "$dest"
git -C "$dest" sparse-checkout set --no-cone /data/ /schema.sql /LICENSE-DATA

if [[ -z "$ref" ]]; then
  target="$(git -C "$dest" rev-parse --verify 'HEAD^{commit}')"
elif target="$(git -C "$dest" rev-parse --verify --quiet "origin/${ref}^{commit}")"; then
  :
elif target="$(git -C "$dest" rev-parse --verify --quiet "${ref}^{commit}")"; then
  :
else
  git -C "$dest" fetch --quiet origin "$ref"
  target="$(git -C "$dest" rev-parse --verify 'FETCH_HEAD^{commit}')"
fi
git -C "$dest" -c advice.detachedHead=false checkout --quiet --detach "$target"

for f in data/manifest.json data/cards.ndjson schema.sql LICENSE-DATA; do
  [[ -f "$dest/$f" ]] || { echo "clone is missing $f" >&2; exit 1; }
done
images="$(find "$dest" -path "$dest/.git" -prune -o -type f \( -iname '*.webp' -o -iname '*.png' -o -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.gif' \) -print)"
if [[ -n "$images" ]]; then
  echo "unexpected image files in the checkout (card images must not be fetched):" >&2
  echo "$images" >&2
  exit 1
fi

echo "commit: $(git -C "$dest" log -1 --format='%H %s (%cI)')"
echo "GCGAPI_DIR=$dest"
echo "GCGAPI_COMMIT=$target"
echo "GCGAPI_RETRIEVED_AT=$retrieved_at"
