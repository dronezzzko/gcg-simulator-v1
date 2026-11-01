#!/usr/bin/env bash
# External Python-vs-Rust comparison (docs/port/BENCHMARKS.md, docs/port/EQUIVALENCE.md).
# This is the only timing-based activity in the project; it lives outside the Rust crates.
#
#   scripts/compare_python_vs_rust.sh env
#   scripts/compare_python_vs_rust.sh best-workers python|rust-reference|rust-presets
#   scripts/compare_python_vs_rust.sh python   [--best N]        # Python baselines, measured once
#   scripts/compare_python_vs_rust.sh part1    [--best N]        # Rust reference search, equal work
#   scripts/compare_python_vs_rust.sh part2    [--best N]        # Rust shipped presets
#   scripts/compare_python_vs_rust.sh equivalence [--matches M] [--seeds "1 2 3"] [--formats "bo1 bo3"]
#        [--presets "standard strong"] [--workers "1 4 10"] [--pairs "a:b c:d"]
#   scripts/compare_python_vs_rust.sh report
#
# Results go to $COMPARE_OUT (default build/comparison). Workload (all timing configurations):
#   benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt
#     --format bo3 --seed 1 --out <dir>
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
out="${COMPARE_OUT:-$repo/build/comparison}"
mkdir -p "$out"

py_bin="$repo/.venv/bin/gcg-sim"
rs_sim="$repo/target/release/gcg-sim"
rs_eval="$repo/target/release/gcg-eval"
dut="examples/decks/red-green-zeon.txt"
bench="examples/decks/blue-white-federation.txt"
workload=("$dut" "$bench" --format bo3 --seed 1)

die() { echo "compare: $*" >&2; exit 2; }

need_python() {
  [[ -x "$py_bin" ]] || uv sync --locked
  [[ -x "$py_bin" ]] || die "missing $py_bin"
}

need_rust() {
  [[ -x "$rs_sim" && -x "$rs_eval" ]] || cargo build --release --locked -p gcg-cli -p gcg-eval
  [[ -x "$rs_sim" && -x "$rs_eval" ]] || die "missing Rust binaries in target/release"
}

record_env() {
  {
    echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "cpu: $(sysctl -n machdep.cpu.brand_string)"
    echo "cores: $(sysctl -n hw.ncpu) total, $(sysctl -n hw.perflevel0.logicalcpu) $(sysctl -n hw.perflevel0.name), $(sysctl -n hw.perflevel1.logicalcpu 2>/dev/null || echo 0) $(sysctl -n hw.perflevel1.name 2>/dev/null || echo -)"
    local tier2
    tier2="$(sysctl -n hw.perflevel2.logicalcpu 2>/dev/null || true)"
    if [[ -n "$tier2" ]]; then
      echo "cores (tier 2): $tier2 $(sysctl -n hw.perflevel2.name)"
    fi
    echo "memory_bytes: $(sysctl -n hw.memsize)"
    echo "macos: $(sw_vers -productVersion) ($(sw_vers -buildVersion))"
    echo "python: $("$repo/.venv/bin/python" --version 2>&1 || echo n/a)"
    echo "uv: $(uv --version)"
    echo "rustc: $(rustc --version)"
    echo "llvm: $(rustc --version --verbose | sed -n 's/^LLVM version: //p')"
    echo "hyperfine: $(hyperfine --version)"
    echo "git: $(git rev-parse --short HEAD) ($(git rev-parse --abbrev-ref HEAD))"
  } | tee "$out/env.txt"
}

# measure NAME RUNS CMD... : hyperfine with a fresh --out directory per run.
measure() {
  local name="$1" runs="$2"
  shift 2
  local dir="$out/runs/$name"
  local cmd
  printf -v cmd '%q ' "$@" --out "$dir"
  hyperfine --runs "$runs" --prepare "rm -rf $(printf %q "$dir")" \
    --export-json "$out/$name.json" --command-name "$name" "$cmd"
}

# Long configurations get 3 runs, short ones 5 (protocol in docs/port/BENCHMARKS.md).
cmd_python() {
  local best="$1"
  need_python
  record_env >/dev/null
  measure py-standard-m100-w"$best" 3 "$py_bin" benchmark "${workload[@]}" --ai-preset standard --matches 100 --workers "$best"
  [[ "$best" == 10 ]] || measure py-standard-m100-w10 3 "$py_bin" benchmark "${workload[@]}" --ai-preset standard --matches 100 --workers 10
  measure py-standard-m20-w1 3 "$py_bin" benchmark "${workload[@]}" --ai-preset standard --matches 20 --workers 1
  measure py-strong-m20-w"$best" 3 "$py_bin" benchmark "${workload[@]}" --ai-preset strong --matches 20 --workers "$best"
}

cmd_part1() {
  local best="$1"
  need_rust
  record_env >/dev/null
  measure rs-ref-standard-m100-w"$best" 5 "$rs_eval" reference-benchmark "${workload[@]}" --reference-preset standard --matches 100 --workers "$best"
  [[ "$best" == 10 ]] || measure rs-ref-standard-m100-w10 5 "$rs_eval" reference-benchmark "${workload[@]}" --reference-preset standard --matches 100 --workers 10
  measure rs-ref-standard-m20-w1 5 "$rs_eval" reference-benchmark "${workload[@]}" --reference-preset standard --matches 20 --workers 1
  measure rs-ref-strong-m20-w"$best" 5 "$rs_eval" reference-benchmark "${workload[@]}" --reference-preset strong --matches 20 --workers "$best"
}

cmd_part2() {
  local best="$1"
  need_rust
  record_env >/dev/null
  measure rs-fast-m100-w"$best" 5 "$rs_sim" benchmark "${workload[@]}" --ai-preset fast --matches 100 --workers "$best"
  [[ "$best" == 10 ]] || measure rs-fast-m100-w10 5 "$rs_sim" benchmark "${workload[@]}" --ai-preset fast --matches 100 --workers 10
  measure rs-fast-m20-w1 5 "$rs_sim" benchmark "${workload[@]}" --ai-preset fast --matches 20 --workers 1
  measure rs-standard-m20-w"$best" 3 "$rs_sim" benchmark "${workload[@]}" --ai-preset standard --matches 20 --workers "$best"
}

# One run per candidate worker count on a 20-match screening workload; prints the fastest.
cmd_best_workers() {
  local impl="$1" candidates=(6 8 10)
  local -a base
  case "$impl" in
    python) need_python; base=("$py_bin" benchmark "${workload[@]}" --ai-preset standard --matches 20) ;;
    rust-reference) need_rust; base=("$rs_eval" reference-benchmark "${workload[@]}" --reference-preset standard --matches 20); candidates+=(auto) ;;
    rust-presets) need_rust; base=("$rs_sim" benchmark "${workload[@]}" --ai-preset fast --matches 20); candidates+=(auto) ;;
    *) die "best-workers needs python, rust-reference or rust-presets" ;;
  esac
  local w
  for w in "${candidates[@]}"; do
    measure "best-$impl-w$w" 1 "${base[@]}" --workers "$w"
  done
  python3 - "$out" "$impl" "${candidates[@]}" <<'PY'
import json, sys
out, impl, cands = sys.argv[1], sys.argv[2], sys.argv[3:]
def wall(path):
    return json.load(open(path))["results"][0]["summary"]["time_wall_clock"]["median"]
times = {w: wall(f"{out}/best-{impl}-w{w}.json") for w in cands}
for w, t in times.items():
    print(f"{impl} workers={w}: {t:.1f} s")
print("best:", min(times, key=times.get))
PY
}

# Byte comparison of normalised outputs: Python benchmark vs Rust reference-benchmark.
cmd_equivalence() {
  local matches=10 seeds="1 2 3" formats="bo1 bo3" presets="standard strong" workers="1 4 10"
  local pairs="red-green-zeon:blue-white-federation purple-white-tekkadan:red-green-zeon"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --matches) matches="$2"; shift 2 ;;
      --seeds) seeds="$2"; shift 2 ;;
      --formats) formats="$2"; shift 2 ;;
      --presets) presets="$2"; shift 2 ;;
      --workers) workers="$2"; shift 2 ;;
      --pairs) pairs="$2"; shift 2 ;;
      *) die "unknown equivalence option $1" ;;
    esac
  done
  need_python
  need_rust
  local eq="$out/equivalence" fail=0 total=0
  mkdir -p "$eq"
  local pair a b seed fmt preset w tag pydir
  for pair in $pairs; do
    a="examples/decks/${pair%%:*}.txt"; b="examples/decks/${pair##*:}.txt"
    for seed in $seeds; do
      for fmt in $formats; do
        for preset in $presets; do
          tag="${pair/:/-vs-}-s$seed-$fmt-$preset-m$matches"
          pydir="$eq/$tag/python"
          if [[ ! -f "$pydir/results.json" ]]; then
            rm -rf "$pydir"
            "$py_bin" benchmark "$a" "$b" --matches "$matches" --seed "$seed" --format "$fmt" \
              --ai-preset "$preset" --workers 10 --out "$pydir" >/dev/null
          fi
          python3 scripts/normalize_outputs.py "$pydir" "$eq/$tag/python.norm"
          for w in $workers; do
            total=$((total + 1))
            rm -rf "$eq/$tag/rust-w$w"
            "$rs_eval" reference-benchmark "$a" "$b" --matches "$matches" --seed "$seed" --format "$fmt" \
              --reference-preset "$preset" --workers "$w" --out "$eq/$tag/rust-w$w" >/dev/null
            python3 scripts/normalize_outputs.py "$eq/$tag/rust-w$w" "$eq/$tag/rust-w$w.norm"
            if diff -r "$eq/$tag/python.norm" "$eq/$tag/rust-w$w.norm" >"$eq/$tag/diff-w$w.txt"; then
              echo "IDENTICAL $tag workers=$w"
            else
              echo "DIFFERENT $tag workers=$w (see $eq/$tag/diff-w$w.txt)"
              fail=$((fail + 1))
            fi
          done
        done
      done
    done
  done
  echo "equivalence: $((total - fail))/$total identical"
  [[ "$fail" == 0 ]]
}

cmd_report() {
  python3 - "$out" <<'PY'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
rows = {}
for f in sorted(out.glob("*.json")):
    rows[f.stem] = json.loads(f.read_text())["results"][0]["summary"]["time_wall_clock"]
print("| configuration | runs | median s | min s | max s |")
print("| --- | ---: | ---: | ---: | ---: |")
for name, r in rows.items():
    print(f"| {name} | {r['count']} | {r['median']:.1f} | {r['min']:.1f} | {r['max']:.1f} |")
pairs = []
for name, r in rows.items():
    if not name.startswith("py-"):
        continue
    rest = name[3:]
    preset, cfg = rest.split("-", 1)
    rust = {"standard": [f"rs-ref-standard-{cfg}", f"rs-fast-{cfg}"],
            "strong": [f"rs-ref-strong-{cfg}", f"rs-standard-{cfg}"]}[preset]
    for rn in rust:
        if rn in rows:
            pairs.append((name, rn, r["median"] / rows[rn]["median"]))
if pairs:
    print()
    print("| Python | Rust | speedup (median) |")
    print("| --- | --- | ---: |")
    for p, rn, s in pairs:
        print(f"| {p} | {rn} | {s:.1f}x |")
PY
}

[[ $# -ge 1 ]] || die "usage: $0 env|best-workers|python|part1|part2|equivalence|report"
command="$1"
shift
best=10
if [[ "${1:-}" == "--best" ]]; then best="$2"; shift 2; fi
[[ "$best" =~ ^([0-9]+|auto)$ ]] || die "--best needs a worker count, got '$best'"
case "$command" in
  env) record_env ;;
  best-workers) cmd_best_workers "${1:-}" ;;
  python) cmd_python "$best" ;;
  part1) cmd_part1 "$best" ;;
  part2) cmd_part2 "$best" ;;
  equivalence) cmd_equivalence "$@" ;;
  report) cmd_report ;;
  *) die "unknown command $command" ;;
esac
