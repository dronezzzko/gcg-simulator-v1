#!/usr/bin/env bash
# Profile-guided build of gcg-sim and gcg-eval (docs/dev/PLAN.md §11.4 step 8,
# docs/dev/BENCHMARKS.md "PGO"). Outside the crates; nothing here is timed.
#
#   scripts/build_pgo.sh            # binaries in target/pgo/use/release/{gcg-sim,gcg-eval}
#
# 1. builds both binaries instrumented (-Cprofile-generate) into target/pgo/gen;
# 2. runs a fixed training workload with fixed seeds (never seed 1 of the comparison workload):
#    the reference search at both reference budgets through gcg-eval, and both shipped presets
#    through gcg-sim, over the three example decks;
# 3. merges the profiles with llvm-profdata (rustup component llvm-tools);
# 4. builds both binaries with -Cprofile-use into target/pgo/use.
#
# PGO changes code layout and inlining only: the outputs are byte-identical to a plain release
# build (Rust never contracts floating-point expressions). GCG_TARGET_CPU overrides the
# target-cpu=native of .cargo/config.toml (RUSTFLAGS replaces it), e.g. apple-m2 for a
# distributable build.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
command -v cargo >/dev/null || PATH="$HOME/.cargo/bin:$PATH"
die() { echo "build_pgo: $*" >&2; exit 2; }

cpu="${GCG_TARGET_CPU:-native}"
base="$repo/target/pgo"
data="$base/data"
train="$base/train"
profdata="$(ls "$(rustc --print sysroot)"/lib/rustlib/*/bin/llvm-profdata 2>/dev/null | head -1)"
[[ -x "$profdata" ]] || die "llvm-profdata not found: rustup component add llvm-tools"

rm -rf "$data" "$train"
mkdir -p "$data" "$train"
RUSTFLAGS="-C target-cpu=$cpu -Cprofile-generate=$data" \
  cargo build --release --locked --offline -p gcg-cli -p gcg-eval --target-dir "$base/gen" \
  || die "instrumented build failed"

gen="$base/gen/release"
rgz=examples/decks/red-green-zeon.txt
bwf=examples/decks/blue-white-federation.txt
pwt=examples/decks/purple-white-tekkadan.txt
run() { "$@" >/dev/null 2>&1 || die "training run failed: $*"; }
run "$gen/gcg-eval" reference-benchmark "$rgz" "$bwf" --matches 3 --format bo3 --seed 7 \
  --reference-preset standard --workers 1 --out "$train/ref-standard"
run "$gen/gcg-eval" reference-benchmark "$bwf" "$rgz" --matches 1 --format bo1 --seed 8 \
  --reference-preset strong --workers 1 --out "$train/ref-strong"
run "$gen/gcg-eval" reference-benchmark "$pwt" "$rgz" --matches 2 --format bo1 --seed 9 \
  --reference-preset standard --workers 1 --out "$train/ref-pwt"
run "$gen/gcg-sim" benchmark "$rgz" "$bwf" --matches 2 --format bo1 --seed 10 \
  --ai-preset fast --workers 1 --out "$train/sim-fast"
run "$gen/gcg-sim" benchmark "$pwt" "$bwf" --matches 1 --format bo1 --seed 11 \
  --ai-preset standard --workers 1 --out "$train/sim-standard"

"$profdata" merge -o "$base/merged.profdata" "$data" || die "llvm-profdata merge failed"
RUSTFLAGS="-C target-cpu=$cpu -Cprofile-use=$base/merged.profdata" \
  cargo build --release --locked --offline -p gcg-cli -p gcg-eval --target-dir "$base/use" \
  || die "profile-use build failed"
echo "build_pgo: $base/use/release/gcg-sim and $base/use/release/gcg-eval"
