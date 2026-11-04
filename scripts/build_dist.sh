#!/usr/bin/env bash
# Distributable binaries (docs/port/PLAN.md §11.4 step 5, T-143): gcg-sim, gcg-eval and gcg-diff
# built with the `dist` profile of Cargo.toml for a fixed CPU baseline instead of the
# target-cpu=native of .cargo/config.toml (RUSTFLAGS replaces it). Outside the crates; nothing
# here is timed.
#
#   scripts/build_dist.sh              # apple-m2 baseline: runs on every Apple M2 or later
#   GCG_TARGET_CPU=apple-m5 scripts/build_dist.sh
#
# Output: target/dist/{gcg-sim,gcg-eval,gcg-diff}, with LICENSE-DATA (the ODbL v1.0 of the card
# data, src/gcg_sim/data/gcgapi/LICENSE-DATA) and NOTICE.txt (README.md's "Licences and
# notices" section) next to them (PLAN §2.4). The binaries embed the card data, rules and
# effect IR, so they run without the repository, Python or the network
# (scripts/verify_binary_offline.sh target/dist/gcg-sim). target-cpu changes code generation
# only, never a result: `target/dist/gcg-diff corpus --tier 1 --require-all` gives the same
# 3804/3804 as the native build (T-092).
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
command -v cargo >/dev/null || PATH="$HOME/.cargo/bin:$PATH"
die() { echo "build_dist: $*" >&2; exit 2; }

[[ $# -eq 0 ]] || die "usage: [GCG_TARGET_CPU=CPU] scripts/build_dist.sh"
cpu="${GCG_TARGET_CPU:-apple-m2}"
# rustc --print target-cpus omits LLVM's aliases (apple-m2 among them), so ask rustc itself.
cfg="$(rustc -C "target-cpu=$cpu" --print cfg 2>&1)"
if grep -q "not a recognized processor" <<<"$cfg"; then
  die "rustc does not know target-cpu $cpu"
fi

# Two builds: gcg-diff enables gcg-engine/testkit, which one build would unify into gcg-sim and
# gcg-eval (PLAN A31: the shipped binaries carry no test-only feature).
RUSTFLAGS="-C target-cpu=$cpu" \
  cargo build --profile dist --locked --offline -p gcg-cli -p gcg-eval \
  || die "dist build of gcg-sim and gcg-eval failed"
RUSTFLAGS="-C target-cpu=$cpu" \
  cargo build --profile dist --locked --offline -p gcg-diff \
  || die "dist build of gcg-diff failed"
for bin in gcg-sim gcg-eval gcg-diff; do
  [[ -x "target/dist/$bin" ]] || die "target/dist/$bin was not built"
done
cp src/gcg_sim/data/gcgapi/LICENSE-DATA target/dist/LICENSE-DATA
awk '/^## Licences and notices$/ {on = 1; next} on && /^## / {exit} on' README.md \
  | sed '/./,$!d' >target/dist/NOTICE.txt
grep -q "ODbL" target/dist/NOTICE.txt && grep -q "BANDAI" target/dist/NOTICE.txt \
  || die "README.md has no Licences and notices section with the ODbL and BANDAI notices"
echo "build_dist: target/dist/{gcg-sim,gcg-eval,gcg-diff,LICENSE-DATA,NOTICE.txt} (target-cpu=$cpu)"
