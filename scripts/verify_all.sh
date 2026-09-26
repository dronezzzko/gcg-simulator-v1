#!/usr/bin/env bash
# Acceptance criterion 1: the full gate on a clean clone of the committed HEAD (uncommitted
# changes are not included): locked sync, lint, format, types, the full test suite including
# the slow robustness and AI-strength tests, source/golden/conflict checks, build, and the
# offline wheel benchmark.
#
#   scripts/verify_all.sh
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
branch="$(git -C "$repo" rev-parse --abbrev-ref HEAD)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

git clone --quiet --no-hardlinks --branch "$branch" "$repo" "$work/repo"
cd "$work/repo"
echo "verifying $(git rev-parse --short HEAD) on $branch in $work/repo"

uv sync --locked
uv run ruff check .
uv run ruff format --check .
uv run mypy
uv run pytest -m "" -n auto -q
uv run python -m gcg_sim.tools.golden --check
uv run python -m gcg_sim.tools.sources --check
uv run python -m gcg_sim.sources.conflicts --check
uv run python -m gcg_sim.rules.index --check
uv build
scripts/verify_wheel_offline.sh "$(ls dist/gcg_sim-*.whl | tail -n 1)"
echo "verify_all passed"
