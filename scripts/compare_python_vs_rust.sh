#!/usr/bin/env bash
# External Python-vs-Rust comparison (docs/port/BENCHMARKS.md; docs/port/PLAN.md §8.8, §8.9,
# §11.5). This is the only timing-based activity in the project; it lives outside the Rust crates.
#
#   scripts/compare_python_vs_rust.sh env
#   scripts/compare_python_vs_rust.sh best-workers python|rust-reference|rust-presets
#   scripts/compare_python_vs_rust.sh python   [--best N]          # Python baselines, measured once
#   scripts/compare_python_vs_rust.sh part1    [--best N|auto]     # Rust reference search, equal work
#   scripts/compare_python_vs_rust.sh part2    [--best N|auto]     # Rust shipped presets
#   scripts/compare_python_vs_rust.sh part2    [--best N|auto] --candidate FILE --as fast|standard
#                                                                  # a tuning candidate (PLAN §12.8)
#   scripts/compare_python_vs_rust.sh report   [--selftest]
#   scripts/compare_python_vs_rust.sh equivalence [--matches M] [--seeds "1 2 3"] [--formats "bo1 bo3"]
#        [--presets "fast standard strong"] [--workers "1 4 10"] [--pairs "a:b c:d e:f"]
#        [--decision-logs "off on"] [--offset-probe N] [--python-only] [--selftest]
#
# Results go to $COMPARE_OUT (default build/comparison). Workload (all timing configurations):
#   benchmark examples/decks/red-green-zeon.txt examples/decks/blue-white-federation.txt
#     --format bo3 --seed 1 --out <dir>
#
# Timing. `python`, `part1` and `part2` record the worker count they used in best-<impl>.txt
# (python, rust-reference, rust-presets), and every Rust timing records the code digest of the
# binary it timed (<name>.digest). `need_rust` always rebuilds and writes the binaries' digest to
# env.txt. `report` pairs runs by role and exits 1 if any of the eight gating configurations is
# missing, below 2x, or timed with a binary whose digest differs from env.txt.
#
# Equivalence (W-160). Raw report bytes are compared after masking only the three implementation
# fields (scripts/normalize_outputs.py). Every configuration runs with and without
# --decision-log. Each Python baseline is generated under PYTHONHASHSEED=1 and =2 and must be
# byte-identical; a cached baseline is reused only while its stamp (Python reference commit,
# working-tree cleanliness, data lock, Python version and command) matches the current tree.
# --offset-probe N adds the two numbering-shift runs of PLAN §8.9 to every configuration.
# --python-only generates and checks the Python baselines alone (no Rust build or runs).
# --selftest runs the negative controls of the normaliser and the comparison (no binaries).
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
out="${COMPARE_OUT:-$repo/build/comparison}"

py_bin="$repo/.venv/bin/gcg-sim"
py="$repo/.venv/bin/python"
rs_sim="$repo/target/release/gcg-sim"
rs_eval="$repo/target/release/gcg-eval"
# GCG_PGO=1 times the profile-guided binaries of scripts/build_pgo.sh instead (identical
# outputs; BENCHMARKS.md "PGO"); env.txt records which build was timed.
rust_build="release"
if [[ "${GCG_PGO:-0}" == 1 ]]; then
  rust_build="pgo"
  rs_sim="$repo/target/pgo/use/release/gcg-sim"
  rs_eval="$repo/target/pgo/use/release/gcg-eval"
fi
dut="examples/decks/red-green-zeon.txt"
bench="examples/decks/blue-white-federation.txt"
workload=("$dut" "$bench" --format bo3 --seed 1)
python_inputs=(src/gcg_sim pyproject.toml uv.lock data)
rust_digest=""
measure_digest=""
CONFIG_FLOOR="2.0"

die() { echo "compare: $*" >&2; exit 2; }
command -v cargo >/dev/null || PATH="$HOME/.cargo/bin:$PATH"

need_python() {
  [[ -x "$py_bin" && -x "$py" ]] || uv sync --locked
  [[ -x "$py_bin" && -x "$py" ]] || die "missing $py_bin"
}

# The first "code_digest" value anywhere in a JSON document on stdin.
json_code_digest() {
  "$py" -c '
import json, sys
def find(x):
    if isinstance(x, dict):
        if isinstance(x.get("code_digest"), str):
            return x["code_digest"]
        for v in x.values():
            got = find(v)
            if got:
                return got
    elif isinstance(x, list):
        for v in x:
            got = find(v)
            if got:
                return got
    return None
got = find(json.load(sys.stdin))
if not got:
    sys.exit("no code_digest in the JSON")
print(got)
'
}

# Always rebuild, so no stale binary is ever timed; record the binaries' code digest.
need_rust() {
  need_python
  if [[ "$rust_build" == pgo ]]; then
    scripts/build_pgo.sh || die "scripts/build_pgo.sh failed"
  else
    cargo build --release --locked --offline -p gcg-cli -p gcg-eval || die "cargo build failed"
  fi
  [[ -x "$rs_sim" && -x "$rs_eval" ]] || die "missing Rust binaries ($rust_build build)"
  rust_digest="$("$rs_eval" env | json_code_digest)" || die "gcg-eval env reported no code_digest"
}

record_env() {
  mkdir -p "$out"
  local digest="n/a"
  if [[ -n "$rust_digest" ]]; then
    digest="$rust_digest"
  elif [[ -f "$out/env.txt" ]]; then
    digest="$(sed -n 's/^rust_code_digest: //p' "$out/env.txt")"
    [[ -n "$digest" ]] || digest="n/a"
  fi
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
    echo "python: $("$py" --version 2>&1 || echo n/a)"
    echo "uv: $(uv --version)"
    echo "rustc: $(rustc --version)"
    echo "llvm: $(rustc --version --verbose | sed -n 's/^LLVM version: //p')"
    echo "hyperfine: $(hyperfine --version)"
    echo "git: $(git rev-parse --short HEAD) ($(git rev-parse --abbrev-ref HEAD))"
    echo "rust_code_digest: $digest"
    echo "rust_build: $rust_build"
  } >"$out/env.txt.tmp"
  mv "$out/env.txt.tmp" "$out/env.txt"
  cat "$out/env.txt"
}

# measure NAME RUNS CMD... : hyperfine with a fresh --out directory per run. Writes NAME.json and
# NAME.digest (the code digest of the timed binary, or "python").
measure() {
  local name="$1" runs="$2"
  shift 2
  local dir="$out/runs/$name"
  local cmd
  printf -v cmd '%q ' "$@" --out "$dir"
  hyperfine --runs "$runs" --prepare "rm -rf $(printf %q "$dir")" \
    --export-json "$out/$name.json" --command-name "$name" "$cmd"
  printf '%s\n' "$measure_digest" >"$out/$name.digest"
}

record_best() {
  printf '%s\n' "$2" >"$out/best-$1.txt"
}

# Long configurations get 3 runs, short ones 5 (protocol in docs/port/BENCHMARKS.md).
cmd_python() {
  local best="$1"
  [[ "$best" =~ ^[0-9]+$ ]] || die "python needs a numeric --best (Python has no --workers auto)"
  need_python
  record_env >/dev/null
  measure_digest="python"
  record_best python "$best"
  measure py-standard-m100-w"$best" 3 "$py_bin" benchmark "${workload[@]}" --ai-preset standard --matches 100 --workers "$best"
  [[ "$best" == 10 ]] || measure py-standard-m100-w10 3 "$py_bin" benchmark "${workload[@]}" --ai-preset standard --matches 100 --workers 10
  measure py-standard-m20-w1 3 "$py_bin" benchmark "${workload[@]}" --ai-preset standard --matches 20 --workers 1
  measure py-strong-m20-w"$best" 3 "$py_bin" benchmark "${workload[@]}" --ai-preset strong --matches 20 --workers "$best"
}

cmd_part1() {
  local best="$1"
  need_rust
  record_env >/dev/null
  measure_digest="$rust_digest"
  record_best rust-reference "$best"
  measure rs-ref-standard-m100-w"$best" 5 "$rs_eval" reference-benchmark "${workload[@]}" --reference-preset standard --matches 100 --workers "$best"
  [[ "$best" == 10 ]] || measure rs-ref-standard-m100-w10 5 "$rs_eval" reference-benchmark "${workload[@]}" --reference-preset standard --matches 100 --workers 10
  measure rs-ref-standard-m20-w1 5 "$rs_eval" reference-benchmark "${workload[@]}" --reference-preset standard --matches 20 --workers 1
  measure rs-ref-strong-m20-w"$best" 5 "$rs_eval" reference-benchmark "${workload[@]}" --reference-preset strong --matches 20 --workers "$best"
}

cmd_part2() {
  local best="$1"
  shift
  local candidate="" role=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --candidate) [[ $# -ge 2 ]] || die "--candidate needs a FILE"; candidate="$2"; shift 2 ;;
      --as) [[ $# -ge 2 ]] || die "--as needs fast or standard"; role="$2"; shift 2 ;;
      *) die "unknown part2 option $1" ;;
    esac
  done
  if [[ -z "$candidate" ]]; then
    [[ -z "$role" ]] || die "--as is only valid with --candidate"
    need_rust
    record_env >/dev/null
    measure_digest="$rust_digest"
    record_best rust-presets "$best"
    measure rs-fast-m100-w"$best" 5 "$rs_sim" benchmark "${workload[@]}" --ai-preset fast --matches 100 --workers "$best"
    [[ "$best" == 10 ]] || measure rs-fast-m100-w10 5 "$rs_sim" benchmark "${workload[@]}" --ai-preset fast --matches 100 --workers 10
    measure rs-fast-m20-w1 5 "$rs_sim" benchmark "${workload[@]}" --ai-preset fast --matches 20 --workers 1
    measure rs-standard-m20-w"$best" 3 "$rs_sim" benchmark "${workload[@]}" --ai-preset standard --matches 20 --workers "$best"
    return
  fi
  [[ -f "$candidate" ]] || die "no candidate file $candidate"
  [[ "$role" == fast || "$role" == standard ]] || die "part2 --candidate needs --as fast or --as standard"
  local stem
  stem="$(basename "$candidate")"
  stem="${stem%.*}"
  [[ "$stem" =~ ^[A-Za-z0-9_.-]+$ ]] || die "candidate file stem '$stem' must match [A-Za-z0-9_.-]+"
  need_rust
  record_env >/dev/null
  measure_digest="$rust_digest"
  local -a cand=("$rs_eval" preset-benchmark "${workload[@]}" --search-config "$candidate")
  printf '%s\n' "$best" >"$out/best-cand-$role-$stem.txt"
  case "$role" in
    fast)
      # The fast role is held to the screening configurations against Python standard.
      measure rs-cand-fast-"$stem"-m100-w"$best" 5 "${cand[@]}" --matches 100 --workers "$best"
      [[ "$best" == 10 ]] || measure rs-cand-fast-"$stem"-m100-w10 5 "${cand[@]}" --matches 100 --workers 10
      measure rs-cand-fast-"$stem"-m20-w1 5 "${cand[@]}" --matches 20 --workers 1
      ;;
    standard)
      # The standard role is held to the full-search configuration against Python strong.
      measure rs-cand-standard-"$stem"-m20-w"$best" 3 "${cand[@]}" --matches 20 --workers "$best"
      ;;
  esac
}

# One run per candidate worker count on a 20-match screening workload; prints the fastest and
# records it in best-workers-<impl>.txt.
cmd_best_workers() {
  local impl="$1" candidates=(6 8 10)
  local -a base
  case "$impl" in
    python) need_python; measure_digest="python"; base=("$py_bin" benchmark "${workload[@]}" --ai-preset standard --matches 20) ;;
    rust-reference) need_rust; measure_digest="$rust_digest"; base=("$rs_eval" reference-benchmark "${workload[@]}" --reference-preset standard --matches 20); candidates+=(auto) ;;
    rust-presets) need_rust; measure_digest="$rust_digest"; base=("$rs_sim" benchmark "${workload[@]}" --ai-preset fast --matches 20); candidates+=(auto) ;;
    *) die "best-workers needs python, rust-reference or rust-presets" ;;
  esac
  record_env >/dev/null
  local w
  for w in "${candidates[@]}"; do
    measure "best-$impl-w$w" 1 "${base[@]}" --workers "$w"
  done
  "$py" - "$out" "$impl" "${candidates[@]}" <<'PY'
import json, sys
out, impl, cands = sys.argv[1], sys.argv[2], sys.argv[3:]
def wall(path):
    return json.load(open(path))["results"][0]["summary"]["time_wall_clock"]["median"]
times = {w: wall(f"{out}/best-{impl}-w{w}.json") for w in cands}
for w, t in times.items():
    print(f"{impl} workers={w}: {t:.1f} s")
best = min(times, key=times.get)
print("best:", best)
with open(f"{out}/best-workers-{impl}.txt", "w") as f:
    f.write(f"{best}\n")
PY
}

# ---------------------------------------------------------------------------------------------
# report: role-paired speedups (T-158)

report_program() {
  "$py" - "$1" "$CONFIG_FLOOR" <<'PY'
import json, pathlib, re, sys

out = pathlib.Path(sys.argv[1])
floor = float(sys.argv[2])
problems: list[str] = []


def wall(name):
    path = out / f"{name}.json"
    if not path.is_file():
        return None
    doc = json.loads(path.read_text())
    # hyperfine 2.0 keeps the wall-clock statistics in results[0].summary.time_wall_clock.
    return doc["results"][0]["summary"]["time_wall_clock"]


def best(impl):
    """The worker count recorded by `python`/`part1`/`part2`; for a Python baseline measured
    before best-<impl>.txt existed, the fastest count of its best-workers runs."""
    path = out / f"best-{impl}.txt"
    if path.is_file():
        return path.read_text().strip()
    runs = {}
    for f in out.glob(f"best-{impl}-w*.json"):
        w = f.stem[len(f"best-{impl}-w"):]
        runs[w] = json.loads(f.read_text())["results"][0]["summary"]["time_wall_clock"]["median"]
    if impl == "python" and runs:
        return min(runs, key=runs.get)
    return None


env = out / "env.txt"
current = None
if env.is_file():
    for line in env.read_text().splitlines():
        if line.startswith("rust_code_digest: "):
            current = line.split(": ", 1)[1].strip()

rows = {}
for f in sorted(out.glob("*.json")):
    try:
        stats = json.loads(f.read_text())["results"][0]["summary"]["time_wall_clock"]
    except (KeyError, IndexError, TypeError, json.JSONDecodeError):
        continue
    rows[f.stem] = stats
print("| configuration | runs | median s | min s | max s |")
print("| --- | ---: | ---: | ---: | ---: |")
for name, r in rows.items():
    print(f"| {name} | {r['count']} | {r['median']:.1f} | {r['min']:.1f} | {r['max']:.1f} |")

bp, br, bs = best("python"), best("rust-reference"), best("rust-presets")
for impl, b in (("python", bp), ("rust-reference", br), ("rust-presets", bs)):
    if b is None:
        problems.append(f"no recorded best worker count for {impl} (best-{impl}.txt)")


def configs():
    py_std_best = f"py-standard-m100-w{bp}"
    py_strong_best = f"py-strong-m20-w{bp}"
    return [
        ("Part 1 A", "headline (best workers)", py_std_best, f"rs-ref-standard-m100-w{br}"),
        ("Part 1 A", "headline (10 workers)", "py-standard-m100-w10", "rs-ref-standard-m100-w10"),
        ("Part 1 A", "single-thread", "py-standard-m20-w1", "rs-ref-standard-m20-w1"),
        ("Part 1 B", "full search (best workers)", py_strong_best, f"rs-ref-strong-m20-w{br}"),
        ("Part 2 fast", "headline (best workers)", py_std_best, f"rs-fast-m100-w{bs}"),
        ("Part 2 fast", "headline (10 workers)", "py-standard-m100-w10", "rs-fast-m100-w10"),
        ("Part 2 fast", "single-thread", "py-standard-m20-w1", "rs-fast-m20-w1"),
        ("Part 2 standard", "full search (best workers)", py_strong_best, f"rs-standard-m20-w{bs}"),
    ]


def digest_of(name):
    path = out / f"{name}.digest"
    return path.read_text().strip() if path.is_file() else None


print()
print("| part | configuration | Python | Rust | Python median s | Rust median s | speedup | status |")
print("| --- | --- | --- | --- | ---: | ---: | ---: | --- |")
for part, label, pn, rn in configs():
    p, r = wall(pn), wall(rn)
    status = "ok"
    speed = ""
    if p is None or r is None:
        status = "MISSING " + " and ".join(n for n, x in ((pn, p), (rn, r)) if x is None)
        problems.append(f"{part} {label}: missing {pn if p is None else rn}")
    else:
        s = p["median"] / r["median"]
        speed = f"{s:.2f}x"
        d = digest_of(rn)
        if current in (None, "n/a"):
            status = "STALE (no rust_code_digest in env.txt)"
            problems.append(f"{part} {label}: env.txt has no rust_code_digest")
        elif d != current:
            status = f"STALE (timed digest {d}, env.txt {current})"
            problems.append(f"{part} {label}: {rn} was timed with digest {d}, env.txt has {current}")
        elif s < floor:
            status = f"BELOW {floor:g}x"
            problems.append(f"{part} {label}: {pn} / {rn} = {s:.2f}x < {floor:g}x")
    pm = f"{p['median']:.1f}" if p else "-"
    rm = f"{r['median']:.1f}" if r else "-"
    print(f"| {part} | {label} | {pn} | {rn} | {pm} | {rm} | {speed} | {status} |")

cand = sorted(n for n in rows if n.startswith("rs-cand-"))
if cand:
    print()
    print("Tuning candidates (part2 --candidate; informational, not gating):")
    print()
    print("| candidate run | Python | speedup | meets the floor |")
    print("| --- | --- | ---: | --- |")
    for rn in cand:
        m = re.fullmatch(r"rs-cand-(fast|standard)-(.+)-(m100-w[^-]+|m20-w[^-]+)", rn)
        if not m:
            continue
        role, stem, cfg = m.groups()
        cb = (out / f"best-cand-{role}-{stem}.txt")
        cbest = cb.read_text().strip() if cb.is_file() else None
        if role == "standard":
            pn = f"py-strong-m20-w{bp}"
        elif cfg == f"m100-w{cbest}" and cbest != "10":
            pn = f"py-standard-m100-w{bp}"
        else:
            pn = f"py-standard-{cfg}"
        p, r = wall(pn), rows[rn]
        if p is None:
            print(f"| {rn} | {pn} (missing) | - | - |")
            continue
        s = p["median"] / r["median"]
        print(f"| {rn} | {pn} | {s:.2f}x | {'yes' if s >= floor else 'no'} |")

print()
if problems:
    for msg in problems:
        print(f"report: {msg}", file=sys.stderr)
    print(f"report: FAIL ({len(problems)} problem(s))")
    sys.exit(1)
print(f"report: all 8 configurations present, current and at least {floor:g}x")
PY
}

cmd_report() {
  if [[ "${1:-}" == "--selftest" ]]; then
    report_selftest
    return
  fi
  [[ $# -eq 0 ]] || die "unknown report option $1"
  need_python
  report_program "$out"
}

# Fixture hyperfine 2.0 files: NAME MEDIAN [DIGEST]
fixture_run() {
  local dir="$1" name="$2" median="$3" digest="${4:-}"
  "$py" - "$dir/$name.json" "$name" "$median" <<'PY'
import json, sys
path, name, median = sys.argv[1], sys.argv[2], float(sys.argv[3])
stats = {"unit": "second", "count": 3, "mean": median, "stddev": 0.5,
         "median": median, "min": median - 1.0, "max": median + 1.0}
doc = {"schema_version": 2,
       "metadata": {"hyperfine_version": "2.0.0", "start_time": "2026-11-01T00:00:00Z",
                    "platform": {"os": "Darwin", "architecture": "arm64"}},
       "primary_metric": "time_wall_clock",
       "results": [{"command": name, "name": name, "measurements": [],
                    "summary": {"time_wall_clock": stats}}]}
open(path, "w").write(json.dumps(doc, indent=2) + "\n")
PY
  if [[ -n "$digest" ]]; then printf '%s\n' "$digest" >"$dir/$name.digest"; fi
}

# A complete, passing set: Python best 6, Rust reference best 8, Rust presets best auto.
report_fixture() {
  local dir="$1"
  rm -rf "$dir"
  mkdir -p "$dir"
  printf 'git: test\nrust_code_digest: d1\n' >"$dir/env.txt"
  printf '6\n' >"$dir/best-python.txt"
  printf '8\n' >"$dir/best-rust-reference.txt"
  printf 'auto\n' >"$dir/best-rust-presets.txt"
  fixture_run "$dir" py-standard-m100-w6 900 python
  fixture_run "$dir" py-standard-m100-w10 970 python
  fixture_run "$dir" py-standard-m20-w1 1270 python
  fixture_run "$dir" py-strong-m20-w6 360 python
  fixture_run "$dir" rs-ref-standard-m100-w8 90 d1
  fixture_run "$dir" rs-ref-standard-m100-w10 95 d1
  fixture_run "$dir" rs-ref-standard-m20-w1 120 d1
  fixture_run "$dir" rs-ref-strong-m20-w8 40 d1
  fixture_run "$dir" rs-fast-m100-wauto 80 d1
  fixture_run "$dir" rs-fast-m100-w10 85 d1
  fixture_run "$dir" rs-fast-m20-w1 110 d1
  fixture_run "$dir" rs-standard-m20-wauto 170 d1
}

# T-158: role pairing, and exit 1 on a missing, below-2x or stale-build configuration.
report_selftest() {
  need_python
  local work
  work="$(mktemp -d "${TMPDIR:-/tmp}/compare-report-selftest.XXXXXX")"
  local failed=0 total=0
  check() {
    local label="$1" want="$2" dir="$3" pattern="${4:-}"
    total=$((total + 1))
    local got=0
    report_program "$dir" >"$dir/report.txt" 2>&1 || got=$?
    local ok=1
    if [[ "$want" == pass && "$got" != 0 ]]; then ok=0; fi
    if [[ "$want" == fail && "$got" != 1 ]]; then ok=0; fi
    if [[ -n "$pattern" ]] && ! grep -qF -- "$pattern" "$dir/report.txt"; then ok=0; fi
    if [[ "$ok" == 1 ]]; then
      echo "ok   $label (exit $got)"
    else
      echo "FAIL $label (exit $got, expected $want${pattern:+, output containing '$pattern'})"
      sed 's/^/     | /' "$dir/report.txt"
      failed=$((failed + 1))
    fi
  }
  local d
  d="$work/complete"; report_fixture "$d"
  check "complete set passes" pass "$d" "all 8 configurations"
  check "headline-best pairs Python w6 with Rust w8 by role" pass "$d" "| py-standard-m100-w6 | rs-ref-standard-m100-w8 |"
  check "full search pairs Python w6 with Rust wauto by role" pass "$d" "| py-strong-m20-w6 | rs-standard-m20-wauto |"
  check "single-thread pairs w1 with w1" pass "$d" "| py-standard-m20-w1 | rs-fast-m20-w1 |"
  d="$work/missing"; report_fixture "$d"; rm "$d/rs-fast-m20-w1.json"
  check "a missing configuration fails" fail "$d" "MISSING rs-fast-m20-w1"
  d="$work/missing-python"; report_fixture "$d"; rm "$d/py-strong-m20-w6.json"
  check "a missing Python baseline fails" fail "$d" "MISSING py-strong-m20-w6"
  d="$work/slow"; report_fixture "$d"; fixture_run "$d" rs-ref-strong-m20-w8 181 d1
  check "a configuration below 2x fails" fail "$d" "BELOW 2x"
  d="$work/edge"; report_fixture "$d"; fixture_run "$d" rs-ref-strong-m20-w8 180 d1
  check "exactly 2x passes" pass "$d" "2.00x"
  d="$work/stale"; report_fixture "$d"; fixture_run "$d" rs-standard-m20-wauto 170 d0
  check "a run timed with another build fails" fail "$d" "STALE (timed digest d0"
  d="$work/nodigest"; report_fixture "$d"; rm "$d/rs-ref-standard-m20-w1.digest"
  check "a run without a recorded digest fails" fail "$d" "STALE (timed digest None"
  d="$work/noenv"; report_fixture "$d"; printf 'git: test\n' >"$d/env.txt"
  check "env.txt without a Rust digest fails" fail "$d" "no rust_code_digest"
  d="$work/nobest"; report_fixture "$d"; rm "$d/best-rust-presets.txt"
  check "a missing best count fails" fail "$d" "no recorded best worker count for rust-presets"
  d="$work/phase0"; report_fixture "$d"; rm "$d/best-python.txt"
  fixture_run "$d" best-python-w6 250 python
  fixture_run "$d" best-python-w8 240 python
  fixture_run "$d" best-python-w10 221 python
  fixture_run "$d" py-standard-m100-w10 970 python
  fixture_run "$d" py-strong-m20-w10 356 python
  check "a Phase 0 Python baseline infers best 10 from best-workers runs" pass "$d" "| py-strong-m20-w10 | rs-ref-strong-m20-w8 |"
  d="$work/candidate"; report_fixture "$d"; printf '8\n' >"$d/best-cand-fast-c1.txt"
  fixture_run "$d" rs-cand-fast-c1-m100-w8 100 d1
  fixture_run "$d" rs-cand-fast-c1-m100-w10 600 d1
  check "candidate rows pair with Python by role and do not gate" pass "$d" "| rs-cand-fast-c1-m100-w10 | py-standard-m100-w10 | 1.62x | no |"
  rm -rf "$work"
  echo "report selftest: $((total - failed))/$total passed"
  [[ "$failed" == 0 ]]
}

# ---------------------------------------------------------------------------------------------
# equivalence (W-160, PLAN §8.8 item 4, §8.9)

# What a cached Python run depends on. A run is reused only while this text is unchanged.
python_stamp() {
  local commit dirty lock
  commit="$(git log -1 --format=%H -- "${python_inputs[@]}")"
  if [[ -n "$(git status --porcelain --untracked-files=all -- "${python_inputs[@]}")" ]]; then
    dirty="yes"
  else
    dirty="no"
  fi
  lock="$(shasum -a 256 data/SOURCES.lock.json | cut -d' ' -f1)"
  printf 'python_commit: %s\npython_dirty: %s\ndata_lock: %s\npython: %s\n' \
    "$commit" "$dirty" "$lock" "$("$py" --version 2>&1)"
}

# baseline_current DIR STAMP: DIR holds a complete run made under STAMP from a clean tree. The
# stamp lives next to the run (DIR.stamp), so it never enters the compared bytes.
baseline_current() {
  local dir="$1" stamp="$2"
  [[ -f "$dir/results.json" && -f "$dir.stamp" ]] || return 1
  grep -qx 'python_dirty: no' <<<"$stamp" || return 1
  [[ "$(cat "$dir.stamp")" == "$stamp" ]]
}

# python_run DIR STAMP ENV... -- ARGS... : a Python benchmark into DIR unless DIR is current.
# Sets run_status to "reused" or "generated".
run_status=""
python_run() {
  local dir="$1" stamp="$2"
  shift 2
  local -a envs=()
  while [[ "$1" != "--" ]]; do envs+=("$1"); shift; done
  shift
  local full
  full="$(printf '%s\ncommand: %s\n' "$stamp" "$(printf '%q ' "${envs[@]}" "$@")")"
  if baseline_current "$dir" "$full"; then
    run_status="reused"
    return
  fi
  rm -rf "$dir" "$dir.stamp"
  env "${envs[@]}" "$py_bin" benchmark "$@" --out "$dir" >/dev/null || die "Python run failed: $dir"
  printf '%s\n' "$full" >"$dir.stamp"
  run_status="generated"
}

normalize() {
  "$py" scripts/normalize_outputs.py "$1" "$2"
}

# compare_norm A B DIFF: byte comparison of two normalised trees; the diff goes to DIFF.
compare_norm() {
  diff -r "$1" "$2" >"$3" 2>&1
}

equivalence_defaults() {
  eq_matches=10
  eq_seeds="1 2 3"
  eq_formats="bo1 bo3"
  eq_presets="fast standard strong"
  eq_workers="1 4 10"
  eq_pairs="red-green-zeon:blue-white-federation purple-white-tekkadan:red-green-zeon blue-white-federation:purple-white-tekkadan"
  eq_logs="off on"
}

cmd_equivalence() {
  equivalence_defaults
  local probe="" selftest=0 python_only=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --matches) eq_matches="$2"; shift 2 ;;
      --seeds) eq_seeds="$2"; shift 2 ;;
      --formats) eq_formats="$2"; shift 2 ;;
      --presets) eq_presets="$2"; shift 2 ;;
      --workers) eq_workers="$2"; shift 2 ;;
      --pairs) eq_pairs="$2"; shift 2 ;;
      --decision-logs) eq_logs="$2"; shift 2 ;;
      --offset-probe) probe="$2"; shift 2 ;;
      --selftest) selftest=1; shift ;;
      --python-only) python_only=1; shift ;;
      *) die "unknown equivalence option $1" ;;
    esac
  done
  if [[ "$selftest" == 1 ]]; then
    equivalence_selftest
    return
  fi
  local log
  for log in $eq_logs; do
    [[ "$log" == off || "$log" == on ]] || die "--decision-logs takes off and/or on, got '$log'"
  done
  if [[ -n "$probe" ]]; then
    [[ "$probe" =~ ^[1-9][0-9]*$ ]] || die "--offset-probe needs a positive offset, got '$probe'"
    [[ -f port/oracle/offset_hook/sitecustomize.py ]] || die "--offset-probe needs port/oracle/offset_hook/sitecustomize.py"
  fi
  if [[ "$python_only" == 1 ]]; then
    need_python
  else
    need_rust
  fi
  mkdir -p "$out"
  local eq="$out/equivalence" stamp
  mkdir -p "$eq"
  stamp="$(python_stamp)"
  local fail=0 total=0 seedfail=0 baselines=0 generated=0 probe_diff=0 workers_diff=0
  local tally="$eq/tally.txt"
  : >"$tally"
  local pair a b seed fmt preset w tag dir target
  for pair in $eq_pairs; do
    a="examples/decks/${pair%%:*}.txt"; b="examples/decks/${pair##*:}.txt"
    [[ -f "$a" && -f "$b" ]] || die "pair $pair: no deck file $a or $b"
    for seed in $eq_seeds; do
      for fmt in $eq_formats; do
        for preset in $eq_presets; do
          for log in $eq_logs; do
            tag="${pair/:/-vs-}-s$seed-$fmt-$preset-m$eq_matches-log-$log"
            dir="$eq/$tag"
            mkdir -p "$dir"
            local -a args=("$a" "$b" --matches "$eq_matches" --seed "$seed" --format "$fmt")
            local -a extra=()
            [[ "$log" == on ]] && extra=(--decision-log)
            baselines=$((baselines + 1))
            local h
            for h in 1 2; do
              python_run "$dir/python-h$h" "$stamp" PYTHONHASHSEED="$h" \
                -- "${args[@]}" --ai-preset "$preset" --workers 10 ${extra[@]+"${extra[@]}"}
              [[ "$run_status" == reused ]] || generated=$((generated + 1))
              normalize "$dir/python-h$h" "$dir/python-h$h.norm"
            done
            if ! compare_norm "$dir/python-h1.norm" "$dir/python-h2.norm" "$dir/diff-hashseed.txt"; then
              echo "PYTHON-HASHSEED-DIFFERS $tag (see $dir/diff-hashseed.txt)"
              seedfail=$((seedfail + 1))
              continue
            fi
            target="$dir/python-h1.norm"
            if [[ -n "$probe" ]]; then
              python_run "$dir/python-off$probe" "$stamp" PYTHONHASHSEED=1 \
                PYTHONPATH="$repo/port/oracle/offset_hook" GCG_ORACLE_EFFECT_OFFSET="$probe" \
                -- "${args[@]}" --ai-preset "$preset" --workers 10 ${extra[@]+"${extra[@]}"}
              normalize "$dir/python-off$probe" "$dir/python-off$probe.norm"
              if compare_norm "$target" "$dir/python-off$probe.norm" "$dir/diff-offset.txt"; then
                echo "PROBE-EQUAL $tag offset=$probe"
              else
                echo "PROBE-DIFFERS $tag offset=$probe (see $dir/diff-offset.txt; the offset run becomes the target)"
                probe_diff=$((probe_diff + 1))
                target="$dir/python-off$probe.norm"
              fi
              python_run "$dir/python-w1" "$stamp" PYTHONHASHSEED=1 \
                -- "${args[@]}" --ai-preset "$preset" --workers 1 ${extra[@]+"${extra[@]}"}
              normalize "$dir/python-w1" "$dir/python-w1.norm"
              if ! compare_norm "$dir/python-h1.norm" "$dir/python-w1.norm" "$dir/diff-python-workers.txt"; then
                echo "PYTHON-WORKERS-DIFFER $tag (Python --workers 1 vs 10; see $dir/diff-python-workers.txt)"
                workers_diff=$((workers_diff + 1))
              fi
            fi
            [[ "$python_only" == 0 ]] || continue
            for w in $eq_workers; do
              total=$((total + 1))
              rm -rf "$dir/rust-w$w"
              "$rs_eval" reference-benchmark "${args[@]}" --reference-preset "$preset" --workers "$w" \
                ${extra[@]+"${extra[@]}"} --out "$dir/rust-w$w" >/dev/null
              normalize "$dir/rust-w$w" "$dir/rust-w$w.norm"
              if compare_norm "$target" "$dir/rust-w$w.norm" "$dir/diff-w$w.txt"; then
                echo "IDENTICAL $tag workers=$w"
                echo "IDENTICAL preset=$preset log=$log" >>"$tally"
              else
                echo "DIFFERENT $tag workers=$w (see $dir/diff-w$w.txt)"
                echo "DIFFERENT preset=$preset log=$log" >>"$tally"
                fail=$((fail + 1))
              fi
            done
          done
        done
      done
    done
  done
  local summary="" k
  for k in $eq_presets; do
    summary+=" $k $(grep -c "^IDENTICAL preset=$k " "$tally" || true)/$(grep -c " preset=$k " "$tally" || true);"
  done
  for k in $eq_logs; do
    summary+=" decision-log $k $(grep -c "^IDENTICAL .* log=$k\$" "$tally" || true)/$(grep -c " log=$k\$" "$tally" || true);"
  done
  echo "equivalence: $((total - fail))/$total identical (${summary# })"
  echo "python baselines: $baselines configurations, hash seeds 1 and 2 identical in $((baselines - seedfail)); $generated run(s) generated, the rest reused"
  if [[ -n "$probe" ]]; then
    echo "offset probe ($probe): $probe_diff configuration(s) depend on the numbering; Python --workers 1 vs 10 differ in $workers_diff"
  fi
  [[ "$fail" == 0 && "$seedfail" == 0 ]]
}

# W-160 negative controls: the normaliser, the comparison, stale baselines, cited docs and the
# default matrix. Needs neither binary.
equivalence_selftest() {
  need_python
  local work failed=0 total=0
  work="$(mktemp -d "${TMPDIR:-/tmp}/compare-equivalence-selftest.XXXXXX")"
  result() {
    total=$((total + 1))
    if [[ "$1" == 1 ]]; then echo "ok   $2"; else echo "FAIL $2"; failed=$((failed + 1)); fi
  }
  local ok
  ok=1; "$py" scripts/normalize_outputs.py --selftest "$work/norm" || ok=0
  result "$ok" "normaliser negative controls (normalize_outputs.py --selftest)"
  # The shell comparison path sees what the normaliser produced.
  ok=0; compare_norm "$work/norm/python.norm" "$work/norm/rust-00.norm" "$work/d0.txt" && ok=1
  result "$ok" "compare_norm: other implementation values only -> identical"
  ok=0; compare_norm "$work/norm/python.norm" "$work/norm/rust-02.norm" "$work/d2.txt" || ok=1
  result "$ok" "compare_norm: 1e-5 instead of 1e-05 -> different"
  ok=0; compare_norm "$work/norm/python.norm" "$work/norm/rust-03.norm" "$work/d3.txt" || ok=1
  result "$ok" "compare_norm: missing trailing newline -> different"
  ok=0; compare_norm "$work/norm/python.norm" "$work/norm/rust-04.norm" "$work/d4.txt" || ok=1
  result "$ok" "compare_norm: changed key order -> different"
  # Stale-baseline detection.
  local stamp base="$work/baseline"
  stamp="$(printf 'python_commit: aaa\npython_dirty: no\ndata_lock: l1\npython: Python 3.15.1\ncommand: x')"
  mkdir -p "$base"
  printf '{}\n' >"$base/results.json"
  printf '%s\n' "$stamp" >"$base.stamp"
  ok=0; baseline_current "$base" "$stamp" && ok=1
  result "$ok" "a baseline with the current stamp is reused"
  ok=0; baseline_current "$base" "${stamp/python_commit: aaa/python_commit: bbb}" || ok=1
  result "$ok" "a baseline from another Python commit is regenerated"
  ok=0; baseline_current "$base" "${stamp/data_lock: l1/data_lock: l2}" || ok=1
  result "$ok" "a baseline from another data lock is regenerated"
  ok=0; baseline_current "$base" "${stamp/command: x/command: y}" || ok=1
  result "$ok" "a baseline made with another command is regenerated"
  printf '%s\n' "${stamp/python_dirty: no/python_dirty: yes}" >"$base.stamp"
  ok=0; baseline_current "$base" "${stamp/python_dirty: no/python_dirty: yes}" || ok=1
  result "$ok" "a baseline from a dirty Python tree is never reused"
  rm "$base/results.json"
  printf '%s\n' "$stamp" >"$base.stamp"
  ok=0; baseline_current "$base" "$stamp" || ok=1
  result "$ok" "an incomplete baseline (no results.json) is regenerated"
  ok=0; [[ "$(python_stamp)" == *"python_commit: "* && "$(python_stamp)" == *"data_lock: "* ]] && ok=1
  result "$ok" "the stamp of this tree names the Python commit and the data lock"
  # Every doc path the comparison scripts cite exists.
  local doc missing=""
  for doc in $(grep -ohE 'docs/[A-Za-z0-9_./-]+\.md' scripts/compare_python_vs_rust.sh scripts/normalize_outputs.py | sort -u); do
    [[ -f "$doc" ]] || missing+=" $doc"
  done
  ok=0; [[ -z "$missing" ]] && ok=1
  result "$ok" "every cited doc exists${missing:+ (missing:$missing)}"
  # The default matrix (PLAN §8.8 item 4).
  equivalence_defaults
  local n_pairs n_presets n_seeds n_formats n_workers n_logs
  n_pairs=$(($(wc -w <<<"$eq_pairs"))); n_presets=$(($(wc -w <<<"$eq_presets"))); n_seeds=$(($(wc -w <<<"$eq_seeds")))
  n_formats=$(($(wc -w <<<"$eq_formats"))); n_workers=$(($(wc -w <<<"$eq_workers"))); n_logs=$(($(wc -w <<<"$eq_logs")))
  ok=0
  if [[ " $eq_presets " == *" fast "* && " $eq_logs " == *" on "* && " $eq_logs " == *" off "* && "$n_pairs" == 3 && "$n_logs" == 2 ]]; then ok=1; fi
  result "$ok" "default matrix: presets '$eq_presets', decision logs '$eq_logs', $n_pairs pairs"
  local per_pass=$((n_pairs * n_seeds * n_formats * n_presets))
  ok=0; [[ "$per_pass" == 54 && $((per_pass * n_workers)) == 162 ]] && ok=1
  result "$ok" "default matrix: $per_pass Python baselines and $((per_pass * n_workers)) Rust runs per decision-log pass"
  rm -rf "$work"
  echo "equivalence selftest: $((total - failed))/$total passed"
  [[ "$failed" == 0 ]]
}

[[ $# -ge 1 ]] || die "usage: $0 env|best-workers|python|part1|part2|equivalence|report"
command="$1"
shift
if [[ "$command" == "-h" || "$command" == "--help" ]]; then
  # The usage block at the top of this file.
  sed -n '5,15p' "${BASH_SOURCE[0]}" | sed 's/^#  //; s/^#//'
  exit 0
fi
best=10
if [[ "${1:-}" == "--best" ]]; then
  [[ $# -ge 2 ]] || die "--best needs a worker count"
  best="$2"
  shift 2
fi
[[ "$best" =~ ^([0-9]+|auto)$ ]] || die "--best needs a worker count, got '$best'"
case "$command" in
  env) need_python; record_env ;;
  best-workers) cmd_best_workers "${1:-}" ;;
  python) cmd_python "$best" ;;
  part1) cmd_part1 "$best" ;;
  part2) cmd_part2 "$best" "$@" ;;
  equivalence) cmd_equivalence "$@" ;;
  report) cmd_report "$@" ;;
  *) die "unknown command $command" ;;
esac
