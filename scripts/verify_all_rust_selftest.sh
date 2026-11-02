#!/usr/bin/env bash
# Negative controls of scripts/verify_all_rust.sh (docs/port/WEAKNESSES.md W-118): the gate fails
# when any step fails. Each control is a copy of the gate's prologue (shell options, EXIT trap,
# the subshell that runs the steps, clean clone of HEAD) followed by one injected step, then
# `echo REACHED; exit 0` and the subshell's closing parenthesis. A failing command, a
# failing command inside a pipeline and an unset variable must each stop the copy with a
# non-zero exit before REACHED; the control `true` must reach it. The gate's steps must not be
# guarded with `|| true` or `|| :`, which would let a failing step pass.
#
#   scripts/verify_all_rust_selftest.sh
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gate="$repo/scripts/verify_all_rust.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

prologue_end="$(grep -n '^echo "verifying ' "$gate" | head -n 1 | cut -d: -f1)"
[[ -n "$prologue_end" ]] || { echo "selftest: the gate has no 'verifying' line" >&2; exit 1; }

control() {
  local name="$1" step="$2" want="$3"
  local copy="$tmp/$name.sh"
  head -n "$prologue_end" "$gate" |
    sed "s|^repo=\"\$(cd .*|repo=\"$repo\"|" >"$copy"
  printf '%s\necho REACHED\nexit 0\n)\n' "$step" >>"$copy"
  set +e
  bash "$copy" >"$tmp/$name.out" 2>&1
  local rc=$?
  set -e
  if [[ $want == pass ]]; then
    if [[ $rc != 0 ]] || ! grep -q '^REACHED$' "$tmp/$name.out"; then
      echo "selftest: control '$name' should pass, exit $rc" >&2; cat "$tmp/$name.out" >&2; exit 1
    fi
  elif [[ $rc == 0 ]] || grep -q '^REACHED$' "$tmp/$name.out"; then
    echo "selftest: injected '$name' did not fail the gate (exit $rc)" >&2; exit 1
  fi
  echo "control $name: exit $rc ($want as expected)"
}

control true 'true' pass
control failing-step 'false' fail
control failing-pipeline 'false | cat' fail
control unset-variable 'echo "$VERIFY_ALL_RUST_SELFTEST_UNSET"' fail

if grep -nE '\|\|[[:space:]]*(true|:)([[:space:]]|;|$)' "$gate"; then
  echo "selftest: a gate step is guarded so that it cannot fail" >&2; exit 1
fi
echo "verify_all_rust selftest passed"
