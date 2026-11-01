# Frozen contracts of the Rust port

Workstream S of `docs/port/PLAN.md` §10.2. This file freezes every public **function signature**
the workstreams implement, with its owner and the Python function it ports. The public **types
and traits** are code: `crates/<crate>/src/contracts.rs` (re-exported from the crate root), the
generated `crates/gcg-effects/src/dsl.rs` and `crates/gcg-engine/src/enums.rs`, and the type
definitions in the `gcg-core` modules named below. Only the lead edits those files and this
one.

Status: complete for all eleven crates (step 1: workspace, `gcg-core`, `gcg-data`,
`gcg-effects`; step 2: `gcg-engine`; step 3: `gcg-ai`, `gcg-ai-reference`, `gcg-search`,
`gcg-runner`, `gcg-cli`, `gcg-eval`, `gcg-diff` and the reachability test). Open items are
listed at the end; each names its owner and a proposal.

Conventions:

- `path:line` is relative to `src/gcg_sim/` unless it starts with `docs/`, `tests/`, `crates/`,
  `port/` or `scripts/`. "Inv NN" is `docs/port/inventory/NN-*.md`.
- **Landed** means the body exists and is tested. **Frozen** means the signature is fixed and
  the owner lands the body with its gate. No function is ever committed without a real body
  (no `todo!`, `unimplemented!` or stub; PLAN §10.2).
- **The Python function wins** (PLAN §10.3.4). If an implementer finds that a frozen signature
  disagrees with the Python function it names (parameters, result, line range), the function is
  the contract and the change protocol below applies.
- Fields marked `pub(crate)` in a contract type are the lead's starting layout of an opaque
  type. The owner may change them in its landing diff without the change protocol, because no
  other crate can observe them; the lead applies the change to the contract file at
  integration. Public fields, names, generics, derives and trait impls are frozen.

## Change protocol (PLAN §10.6)

1. A workstream that needs a frozen signature or contract type changed stops that part and
   continues on the parts that do not depend on it.
2. Its report states the proposed change, the reason (with the Python `path:line` when the
   Python function disagrees) and the affected consumers.
3. The lead edits the contract file and this document in one change, records the change in
   the log at the end of this file, and tells every affected workstream.
4. A consumer that needs a provider earlier may write a throwaway shim of the frozen
   signatures in its own worktree. Shims are never handed back; the lead's integration check
   rejects any diff outside the workstream's "Owns" list and any file matching `*shim*`.

## Workspace (S)

| Item | Where | Notes |
| --- | --- | --- |
| Members | root `Cargo.toml` | the eleven crates of PLAN §2.1, listed explicitly (X19) |
| Edges | each `crates/*/Cargo.toml` | PLAN §2.2; `gcg-cli` never depends on `gcg-ai-reference` or `gcg-diff`. Additions: `gcg-search` has a dev-dependency on `gcg-ai-reference` (D2's null-config test; never in a normal build); `gcg-eval` has direct edges to `gcg-effects` and `gcg-engine`, which were already in its closure (head-to-head plays games itself); `gcg-cli` has a direct edge to `gcg-ai`, already in its closure through `gcg-runner` and `gcg-search`, because its agent factory names `gcg_ai::{Agent, AgentError}` (`crates/gcg-cli/tests/factory_types.rs` fails to compile without it) |
| Reachability | `crates/gcg-cli/tests/no_reference.rs` | landed (S, T-022): walks `gcg-cli`'s normal and build edges through every workspace manifest and fails on `gcg-ai-reference` or `gcg-diff` (section gcg-cli below) |
| Features | `gcg-engine`: `testkit`, `paranoid`; `gcg-diff`: `slow`, `paranoid = ["gcg-engine/paranoid"]`; `gcg-search`, `gcg-eval`: `slow`; `gcg-cli`, `gcg-eval`: `mimalloc` (optional dep); `gcg-eval`: `zstd` (optional dep, W-99) | `slow` gates long `[[test]]` targets (A12); `paranoid` is never in a shipped build |
| External crates | `[workspace.dependencies]` | A8 list (`sha2`, `zmij`, `serde_json` with `preserve_order` + `float_roundtrip`, `libm`, `hashbrown`, `foldhash`) plus the M1 fetch (PLAN §0.1): `signal-hook` 0.4, `mimalloc` 0.1, `zstd` 0.14. All are in `Cargo.lock` and fetched (`cargo fetch --locked`) |
| Lints | `[workspace.lints]`, `clippy.toml` | `unsafe_code = "deny"`, `clippy::all` (warn, `-D warnings` in the gate); `clippy.toml` bans the float, ordering, wall-clock and hash-container items of PLAN §9.9 |
| Profiles | root `Cargo.toml` | `release` (opt 3, fat LTO, 1 CGU, `panic = "abort"`, line tables); `dist` inherits `release` (PLAN §11.4 step 5) |
| Target CPU | `.cargo/config.toml` | `target-cpu=native` locally; distributable: `RUSTFLAGS="-C target-cpu=apple-m2" cargo build --profile dist` (M6: `native` or `apple-m5`, PLAN §11.6) |
| Binaries | — | `gcg-cli` (bin `gcg-sim`), `gcg-eval` (bin `gcg-eval`) and `gcg-diff` (bin `gcg-diff`) are libraries with no items until their workstream (E, F, G) lands the `[[bin]]` target and `main.rs` with a real program |
| DSL generator | `port/oracle/gen_dsl.py` | writes `crates/gcg-effects/src/dsl.rs`; `--check` exits 1 when it is stale |
| Engine-enum generator | `port/oracle/gen_engine_enums.py` | writes `crates/gcg-engine/src/enums.rs` from `engine/types.py`, the module constants and an AST scan of the call sites; `--check` exits 1 when it is stale |
| Engine test features | `crates/gcg-engine/Cargo.toml` | a self dev-dependency with `testkit`, so the crate's own `tests/**` (C) see the testkit |

Gate commands of this milestone:

```text
export PATH=$HOME/.cargo/bin:$PATH
cargo fmt --all --check && cargo clippy --workspace --all-targets --locked --offline -- -D warnings && cargo test --workspace --locked --offline
uv run python port/oracle/gen_dsl.py --check
uv run python port/oracle/gen_engine_enums.py --check
uv run ruff check port && uv run ruff format --check port && MYPYPATH=src uv run mypy --strict port/oracle
cargo tree -p gcg-cli -e normal --locked --offline | grep -cE 'gcg-ai-reference|gcg-diff'   # prints 0 (grep then exits 1)
```

`MYPYPATH=src` is needed because `gcg_sim` ships no `py.typed`, so mypy skips the installed
package (`import-untyped`) unless it reads the sources. PLAN §13 M2 writes the command without
it (open item O13).

## gcg-core

Owners: S (lead) for `small`, `portable`, `stable_hash`, `prop`, `lookup` and the M0 modules;
E for `pool` and `sys` (PLAN §10.1). Contract types live in the module files themselves; E adds
its functions to `pool.rs`, `sys/topology.rs` and `sys/sigint.rs`.

### Landed in M0 (lead; verified against Python vectors)

```rust
// rng.rs — gcg_sim/rng.py
pub struct SplitMix64 { pub state: u64 }
impl SplitMix64 {
    pub const fn new(seed: u64) -> Self;
    pub const fn next_u64(&mut self) -> u64;
    pub fn randrange(&mut self, n: u64) -> u64;          // rng.py:30-38, panics on n == 0 (ValueError)
    pub fn randrange_usize(&mut self, n: usize) -> usize;
    pub const fn random(&mut self) -> f64;               // (x >> 11) * 2^-53
    pub fn shuffle<T>(&mut self, items: &mut [T]);        // rng.py:43-46
}
pub enum SeedPart<'a> { Int(i64), Str(&'a str) }
pub fn derive_seed(master: u64, path: &[SeedPart<'_>]) -> u64;   // rng.py derive_seed
macro_rules! derive_seed { ($master, $($part),*) }
// pyrepr.rs, pyfloat.rs, pyjson.rs, pymath.rs, libm.rs
pub fn repr_str(s: &str) -> String;                       // ASCII only (see open item O2)
pub fn repr_tuple(items: &[PyRepr<'_>]) -> String;
pub fn repr_f64(x: f64) -> String;
pub fn format_fixed(x: f64, prec: usize) -> String;       // format(x, ".Nf")
pub fn round_f64(x: f64, ndigits: usize) -> f64;          // round(x, n)
pub enum Json { Null, Bool(bool), Int(i128), Float(f64), Str(String), Array(Vec<Json>), Object(Vec<(Key, Json)>) }
pub fn dumps(value: &Json, opts: DumpOptions) -> String;  // json.dumps
pub fn py_sum(values: impl IntoIterator<Item = f64>) -> f64;   // CPython 3.12+ Neumaier sum()
pub const fn floordiv(a: i64, b: i64) -> i64;
pub const fn pymod(a: i64, b: i64) -> i64;
pub fn first_max_by<T, F>(items: impl IntoIterator<Item = T>, cmp: F) -> Option<T>;
pub fn first_min_by<T, F>(items: impl IntoIterator<Item = T>, cmp: F) -> Option<T>;
pub fn cmp_f64(a: f64, b: f64) -> Ordering;
pub fn erfc(x: f64) -> f64;  pub fn pow(x: f64, y: f64) -> f64;  pub fn square(x: f64) -> f64;
pub fn exp(x: f64) -> f64;   pub fn ln(x: f64) -> f64;           pub fn sqrt(x: f64) -> f64;
```

Frozen, owner S, lands before its first consumer (D1): PLAN §2.1 lists them in `pymath`, but
they do not exist yet.

```rust
pub fn py_max2(a: f64, b: f64) -> f64;   // Python max(a, b): `b if b > a else a` (first wins, NaN as CPython)
pub fn py_min2(a: f64, b: f64) -> f64;   // Python min(a, b): `b if b < a else a`
```

### `small`, `lookup`, `stable_hash`, `prop`, `portable` (S; landed in this milestone)

```rust
// small.rs — fixed-capacity inline arrays (PLAN §4.3 "Capacities")
pub struct InlineVec<T, const N: usize> { /* len: u8, items: [T; N] */ }   // Copy if T: Copy
pub struct CapacityError { pub capacity: usize }
impl<T: Copy + Default, const N: usize> InlineVec<T, N> {
    pub fn new() -> Self;
    pub fn try_push(&mut self, item: T) -> Result<(), CapacityError>;   // never a silent drop
    pub fn clear(&mut self);
}
impl<T, const N: usize> InlineVec<T, N> {
    pub const CAPACITY: usize;
    pub const fn len(&self) -> usize;  pub const fn is_empty(&self) -> bool;
    pub fn as_slice(&self) -> &[T];
}
// PartialEq/Eq/Hash compare the stored elements only.

// lookup.rs — the only sanctioned hash map (PLAN §9.9): fixed foldhash state, no iteration
pub struct LookupMap<K, V> { /* hashbrown::HashMap<K, V, FixedState> */ }
impl<K: Hash + Eq, V> LookupMap<K, V> {
    pub fn new() -> Self;  pub fn with_capacity(capacity: usize) -> Self;
    pub fn insert(&mut self, key: K, value: V) -> Option<V>;
    pub fn get<Q>(&self, key: &Q) -> Option<&V> where K: Borrow<Q>, Q: Hash + Eq + ?Sized;
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V> where K: Borrow<Q>, Q: Hash + Eq + ?Sized;
    pub fn contains_key<Q>(&self, key: &Q) -> bool where K: Borrow<Q>, Q: Hash + Eq + ?Sized;
    pub fn len(&self) -> usize;  pub fn is_empty(&self) -> bool;
}
// Debug prints the length only.

// stable_hash.rs — SHA-256 keys, never std::hash (PLAN §12.3 S7)
pub struct StableKey(pub u64);                 // first 8 digest bytes, big-endian (as derive_seed)
pub struct StableHasher { /* Sha256 */ }
impl StableHasher { pub fn new() -> Self; pub fn update(&mut self, bytes: &[u8]); pub fn finish(self) -> StableKey; }
pub fn stable_key(bytes: &[u8]) -> StableKey;

// prop.rs — property harness (PLAN §9.6): case i of `name` uses SplitMix64(derive_seed(TEST_SEED, name, i))
pub const TEST_SEED: u64 = 0x6763_672D_7072_6F70;  // ASCII "gcg-prop"
pub fn case_seed(name: &str, index: u32) -> u64;
pub fn check<F>(name: &str, cases: u32, property: F) where F: FnMut(&mut SplitMix64) -> Result<(), String>;
// panics with the property name, case index, seed and message; no shrinking

// portable.rs — pure-Rust math for the strengthened search only (W-137, X22)
pub use libm::{exp, log};
```

### `pool` (E; types landed, functions frozen) — PLAN §7.1

```rust
pub struct Cancel { /* flag: Arc<AtomicBool> */ }       // set by SIGINT only (A3)
impl Cancel {
    pub fn new() -> Self;                                 // landed
    pub fn is_cancelled(&self) -> bool;                   // landed; relaxed load
    pub(crate) fn flag(&self) -> &Arc<AtomicBool>;        // landed; for sys::sigint::install only
}
pub struct Pool { /* threads: NonZeroUsize */ }
impl Pool {
    pub const fn new(threads: NonZeroUsize) -> Pool;      // landed
    pub const fn threads(&self) -> NonZeroUsize;          // landed
    /// Landed (S, review fixes): runs f once on the calling thread as the only task of a
    /// one-thread run, with a fresh S::default() and a TaskCx whose fork_join runs its jobs
    /// inline in input order (as threads == 1). The way every caller outside map_indexed
    /// builds an AgentCx (gcg-ai section). E keeps it in step with its layout of Shared.
    pub fn run_inline<S, T>(f: impl FnOnce(&mut S, &TaskCx<'_, S>) -> T) -> T
    where S: Default + Send + 'static;
    /// Runs task(i) for i in 0..n; results in index order whatever the schedule. `threads == 1`
    /// runs everything inline in index order. on_done(count) runs on the calling thread after
    /// each completion (counts, never indices; runner/run.py:67-80). The first failure lowers
    /// the run's lowest_failed (fetch_min): workers stop claiming higher indices and abort
    /// running higher ones between decisions; lower indices finish (W-149, runner/run.py:45-53).
    pub fn map_indexed<S, T, E>(
        &self, n: usize, cancel: &Cancel,
        task: impl Fn(usize, &mut S, &TaskCx<'_, S>) -> Result<T, E> + Sync,
        on_done: impl FnMut(usize),
    ) -> Result<Vec<T>, PoolError<E>>
    where S: Default + Send + 'static, T: Send, E: Send;
}
pub enum PoolError<E> { Task { index: usize, error: E }, Cancelled }
pub struct TaskCx<'p, S> { /* private: the run's shared queue and scratch free list */ }
impl<S: Default + Send + 'static> TaskCx<'_, S> {
    /// Fork-join inside a task: queues jobs 1.. on the run's shared queue, runs job 0 itself,
    /// then runs queued jobs (its own first) until all of its jobs are done ("help first",
    /// blocking waits, no spinning or timeouts). Each job borrows a scratch `S` from the free
    /// list. Returns results in input order.
    pub fn fork_join<I, T>(&self, inputs: Vec<I>, f: Arc<dyn Fn(usize, I, &mut S) -> T + Send + Sync>) -> Vec<T>
    where I: Send + 'static, T: Send + 'static;
}
```

`fork_join` differs from PLAN §7.1, which passes `f: &(dyn Fn(usize, I, &mut S) -> T + Sync)`
with non-`'static` inputs: safe Rust cannot hand a borrow of the forking task's stack to a pool
worker that is already running (the scoped threads' lifetime is the whole run), so jobs must own
their inputs and closure. The two callers fit: play/draw simulations (`ai/setup.py:86-111`) take
an owned state clone and a seed, and root trees (§12.3 S1) take owned states. See open item O1.

### `sys` (E; types landed, functions frozen) — PLAN A3, A4, §7.4, §7.5, §11.6

```rust
// sys/sigint.rs
/// signal_hook::flag::register(SIGINT, cancel.flag()); safe code, no unsafe block.
pub fn install(cancel: &Cancel) -> std::io::Result<()>;

// sys/topology.rs (types landed: PerfLevel, TopologySource, Topology, SYSCTL)
/// Runs `/usr/sbin/sysctl -n` for hw.nperflevels, hw.perflevelN.{logicalcpu,physicalcpu,name},
/// hw.cachelinesize and the `hw.optional.arm` listing; falls back to available_parallelism().
pub fn detect() -> Topology;
/// The parser behind detect(): `query("hw.nperflevels")` etc. returns the trimmed `sysctl -n`
/// value, `arm_listing` the `sysctl hw.optional.arm` text. Unit-tested on captured two-tier
/// and three-tier outputs. None when a required key is missing or malformed.
pub fn parse_sysctl(query: &dyn Fn(&str) -> Option<String>, arm_listing: &str) -> Option<Topology>;
impl Topology {
    /// --workers auto: max(1, min(self.logical_cpus, matches)) (PLAN §7.4).
    pub fn auto_workers(&self, matches: usize) -> usize;
}
```

## gcg-data (owner A) — PLAN §10.3.1, Inv 03

Types (`crates/gcg-data/src/contracts.rs`): `DefId`, `CardType`, `Color`, `LinkCondition`,
`CardDef`, `AppliedOverride`, `CardDb`, `DataError`, `DeckList`, `DeckEntry`, `CardCount`,
`Deck`, `ViolationCode` (13 codes with their exact message formats), `Violation`, `DeckError`,
`ImplementationStatus` (trait), `ValidateOptions`, `ConstructionRules`, `RuleException`,
`Restricted`, `BannedPair`, `AttributePairRule`, `Banlist`, `RulesVersion`, `Bo3Rules`,
`DataStatusParts`, and the constants `EX_BASE_NUMBER`, `EX_RESOURCE_NUMBER`,
`UNIT_VARIANT_PREFIX`, `SYNTHETIC_PREFIX`, `NOT_APPLIED`, `PACKAGE`, `RESULTS_SCHEMA_VERSION`,
`RESULTS_SCHEMA_ID`, `MAIN_DECK_TYPES`, `MAX_EXPANDED_CARDS`. Landed trivial accessors:
`CardType::{ALL, as_str, from_value, is_token, is_unit, is_base, is_resource}`,
`Color::{ALL, as_str, from_value}`, `CardDef::{names, is_token, is_unit_card,
is_pilot_capable}`, `ViolationCode::{ALL, as_str}`.

Embedded-data accessors (`CardDb::embedded`, `official::*`) return `&'static` values: the
embedded bytes are fixed at compile time and `cargo test -p gcg-data` loads them, so a failure
there is a build defect. Anything loaded from other sources returns `Result` (W-157).

```rust
impl CardDb {
    pub fn embedded() -> &'static CardDb;                                   // cards/db.py:327-329 get_card_db (OnceLock)
    pub fn from_sources(cards_ndjson: &str, overrides: &str) -> Result<CardDb, DataError>;
                                                                            // cards/db.py:57-69, :116-158; W-157: no empty default
    pub fn len(&self) -> usize;                                             // :283-284 (1153)
    pub fn defs(&self) -> &[CardDef];                                       // :286-287 __iter__, def_id order
    pub fn by_id(&self, id: DefId) -> &CardDef;                             // :289-290
    pub fn get(&self, number_or_product_id: &str) -> Option<&CardDef>;      // :292-297 (Python strip(), case-sensitive)
    pub fn normalize_id(&self, s: &str) -> Option<&str>;                    // :305-307
    pub fn real_cards(&self) -> impl Iterator<Item = &CardDef>;             // :309-313 (1152)
    pub fn unit_variant(&self, id: DefId) -> Option<&CardDef>;              // :274-277
    pub fn base_def_id(&self, id: DefId) -> DefId;                          // :279-281
    pub fn token_for_key(&self, key: &str) -> Option<&CardDef>;             // :315-316 token_for(spec) = _token_by_key[spec.key]
    pub fn ex_base(&self) -> &CardDef;                                      // :318-320
    pub fn ex_resource(&self) -> &CardDef;                                  // :322-324
    pub fn applied_overrides(&self) -> &[AppliedOverride];                  // :124-146
    pub fn fingerprints(&self) -> [String; 4];                              // Inv 03 §9 (carddefs, def_ids, token_keys, applied_overrides)
}
impl CardDef {
    pub fn is_vanilla(&self) -> bool;                                       // cards/model.py:139-141 (Python str.strip whitespace)
}
impl fmt::Display for DataError;  impl std::error::Error for DataError;    // texts of the cited Python errors

// Decks — deck/parse.py, deck/model.py, deck/validate.py
pub fn load_deck(path: &Path) -> Result<Deck, DeckError>;                   // deck/parse.py:111-114 (utf-8-sig, universal newlines, stem)
pub fn parse_deck(text: &str, name: &str, source: Option<&str>) -> Result<Deck, DeckError>;
                                                                            // :72-108 (grammar :18-69, build_deck :92-108); W-144 counts
pub fn resolve_card_number<'a>(written: &str, db: &'a CardDb) -> Option<&'a str>;   // :23-42
pub fn deck_digest(deck: &Deck) -> String;                                  // :121-129
/// Expands the run-length lists in order (Python's sorted tuples); DeckError::TooLarge above
/// MAX_EXPANDED_CARDS (W-144). Numbers of a parsed deck are always known.
pub fn to_decklist(deck: &Deck) -> Result<DeckList, DeckError>;             // :117-118
pub fn validate(deck: &Deck, opts: ValidateOptions<'_>) -> Vec<Violation>;  // deck/validate.py:268-289, Python order
pub fn require_legal(deck: &Deck, implemented: &dyn ImplementationStatus) -> Result<(), DeckError>;  // :292-300
impl fmt::Display for DeckError;  impl std::error::Error for DeckError;    // deck/model.py:55-58, cli.py:102-110

pub mod official {                                                          // deck/official.py
    pub fn construction_rules() -> &'static ConstructionRules;              // :74-86
    pub fn banlist() -> &'static Banlist;                                   // :112-127 (checks every predicate operator)
    pub fn matches(pred: &serde_json::Value, card: &CardDef, matched: Option<&str>) -> bool;   // :135-173, JSON key order
    pub fn rules_version() -> &'static RulesVersion;                        // reports/versions.py:58,66-67
    pub fn bo3_rules() -> &'static Bo3Rules;                                // reports/versions.py:72-74
}
pub mod names {                                                             // deck/names.py (tables from export_unicode.py, A27)
    pub fn strip_invisible(s: &str) -> String;                              // :25-26
    pub fn normalize_name(s: &str) -> String;                               // :29-33 (NFKC, zero-width, quotes, split, casefold)
    pub fn names_match(written: &str, names: &[Box<str>]) -> bool;          // :36-38
}

// Reports support — reports/versions.py, cli.py
pub fn versions(package: &str, code_digest: &str) -> Json;                  // reports/versions.py:55-75 (insertion order)
pub fn conflict_resolutions(decks: &[(&str, &Deck)]) -> Json;               // :116-135; labels as reports/results.py:181-186
pub fn data_status_parts(package: &str, code_digest: &str) -> DataStatusParts;   // cli.py:263-291 without coverage
```

Differences from PLAN §10.3.1, settled here: `Deck` stores sorted run-length lists (W-144) and
`to_decklist` returns `Result`; `Violation` keeps Python's `cards`; `DeckError::Invalid` keeps
`deck_name` and `problems` (Python's `str(DeckError)` needs both) and `Decode` carries the path;
`validate` takes `ValidateOptions` with an `ImplementationStatus`, because check 11 reads the
effect registry, which `gcg-data` cannot depend on; `conflict_resolutions` takes the deck labels
and `data_status_parts` the version strings; `CardDb::defs` and `resolve_card_number` are added.

## gcg-effects (owner B1) — PLAN §3, §5.1-§5.3, §5.6-§5.7, §10.3.2, Inv 04

Types:

- `crates/gcg-effects/src/dsl.rs`, generated by `port/oracle/gen_dsl.py`: the 13 DSL enums and
  `Duration` (`P`, `Side`, `Loc`, `Op`, `Kw`, `Stat`, `CardKind`, `RuleKind`, `DamageKind`, `Ev`,
  `Timing`, `Where`, `Gate`, `Duration`; declaration order as discriminants, so `Loc as i32` is
  `LOC_CODES`, `view.py:936`), the hook enums `StepHook` (13), `CondHook` (7), `FilterHook` (9),
  `ValueHook` (5) in registration order, one struct per DSL class (137) and instruction class
  (9) with Python's field order, `Node`, `NodeKind`, the alias kind sets
  (`NodeKind::{VALUE, FILTER, REF, COND, CONTINUOUS, COST, STEP, ABILITY, INSTR}`) and the
  schema tables `CLASSES` (147, `Program` last), `ENUMS` (14). Generated accessors:
  `<Enum>::{PY_CLASS, ALL, SORTED_BY_VALUE, as_str, py_name, from_value}` (`SORTED_BY_VALUE` is
  PLAN §5.1's `Kw::sorted_by_value()`), `<Hook>::{ALL, as_str, from_name}`,
  `Node::kind`, `NodeKind::{ALL, py_name, schema}`.
- `crates/gcg-effects/src/contracts.rs`: `SUPPORTED_IR_VERSION`, `IR_REGENERATE_COMMAND`,
  `NodeId`, `ProgramId`, `AbilityId`, `StrId`, `VarId`, `FilterSetId`, `ContentId` (field
  `pub(crate)`: only `from_hex` and the loader build one),
  the validated handles `FilterId`, `RefId`, `SelId`, `CondId`, `StepId`, `ContinuousId`,
  `TriggerId`, `RuleModId`, `CostId`, `AbilityNodeId` (constructed only inside the crate;
  `.node()` gives the `NodeId`), `ValueRef`, `ParamValue`, `Program`, `AbilityEntry`,
  `ScriptSource`, `EvSet`, `KwAmounts`, `CardEntry`, `KeywordProgram`, `SystemProgram`,
  `FilterSet`, `ContRef`, `ContView`, `StepParams`, `CondParams`, `FilterParams`, `ValueParams`,
  `HookParams`, `Registry`, `IrError`.

Field typing of the generated nodes: `int` is `i64`; `bool` is `bool`; `str` is `StrId`, except
frame-variable and loop-key names (`VarSize.var`, `Var.name`, `Choose.var`,
`Choose.distinct_from`, `Discard.var`, `DeployToken.var`, `Mill.var`, `LookTop.var`,
`ForEach.var`, `BindVar.var`, `DelayedTrigger.bind_vars`, `LoopInit.key`, `LoopNext.key`,
`LoopNext.var`), which are `VarId`, and `Custom*.name`, which is the hook enum; `Value` is
`ValueRef`; every other node type is its handle; `X | None` is `Option<X>`; `tuple[X, ...]` is
`Box<[X]>`; params are `Box<[(StrId, ParamValue)]>`.

```rust
impl Registry {
    pub fn embedded() -> Arc<Registry>;                                    // registry.py:400-402 get_registry; IR via include_str!
    pub fn from_ir(ir_json: &str, db: &'static CardDb) -> Result<Registry, IrError>;   // PLAN §3.3-§3.4 (schema check vs dsl::CLASSES/ENUMS)
    pub fn with_fragment(&self, fragment: &serde_json::Value) -> Result<Registry, IrError>;
                                                                           // registry.py:209-220 override_script; PLAN §3.4 test overrides
    pub fn db(&self) -> &'static CardDb;                                   // Registry.db (registry.py:70)
    pub fn generation(&self) -> u32;                                       // changes on with_fragment (W-33)
    pub fn node(&self, id: NodeId) -> &Node;
    pub fn program(&self, id: ProgramId) -> &Program;                      // Registry.programs
    pub fn ability(&self, aid: AbilityId) -> &AbilityEntry;                // Registry.abilities
    pub fn card(&self, def: DefId) -> &CardEntry;                          // registry.py:140-141
    pub fn str(&self, id: StrId) -> &str;
    pub fn var_name(&self, v: VarId) -> &str;
    pub fn keyword_program(&self, kw: KeywordProgram) -> ProgramId;        // registry.py:327-329 (0, 1, 2)
    pub fn system_program(&self, s: SystemProgram) -> ProgramId;           // interp.py:1581-1594 (fixed 623, 624, 625)
    pub fn aliases(&self, def: DefId) -> &[StrId];                         // registry.py:240-241
    pub fn always_events(&self) -> EvSet;                                  // registry.py:356-378
    pub fn events_of(&self, def: DefId) -> EvSet;                          // events_by_def, registry.py:356-397
    pub fn hp_reduction_possible(&self) -> bool;                           // registry.py:356-378
    pub fn hook_params(&self, node: NodeId) -> Option<&HookParams>;        // typed params of a Custom* node (PLAN §5.3)
    pub fn cont_of(&self, n: ContinuousId) -> ContRef;                     // canonical ContRef (PLAN §5.6, registry.py:124-130)
    pub fn cont_node(&self, c: ContRef) -> ContView<'_>;                   // typed read access for view._apply_cont (view.py:347-406)
    pub fn filter_set(&self, f: FilterSetId) -> &FilterSet;                // registry.py:132-138
    pub fn program_by_content_id(&self, cid: &ContentId) -> Option<ProgramId>;   // PLAN §4.8
    pub fn cont_by_content_id(&self, cid: &ContentId) -> Option<ContRef>;       // PLAN §4.8
    pub fn filter_set_by_content_id(&self, cid: &ContentId) -> Option<FilterSetId>;   // PLAN §4.8
    pub fn node_content_id(&self, id: NodeId) -> ContentId;                // runner/digest.py:26-27
    pub fn filter_set_content_id(&self, f: FilterSetId) -> ContentId;      // runner/digest.py:40-48
    pub fn rule_mod_repr(&self, rule: RuleModId) -> &str;                  // ai/actions.py:360 sort key
    pub fn repr(&self, id: NodeId) -> String;                              // CPython repr (PLAN §5.7)
    pub fn golden_json(&self, def: DefId) -> Json;                         // effects/golden.py:19-51
    // added in step 2 (change log 2026-11-01, open item O6): reverse lookups the engine needs
    pub fn var_id(&self, name: &str) -> Option<VarId>;                    // IR var_names; the engine's literal names (card, pilot, unit, __todeck, __play, looked, t) resolve once per game
    pub fn str_id(&self, s: &str) -> Option<StrId>;                       // IR strings; AbilityGrant card numbers (interp.py:1549), canon::from_canonical
    pub fn filters_key(&self, filters: &[FilterId]) -> Option<FilterSetId>;   // registry.py:132-138 (an ApplyPlayer filter tuple; None only outside the IR)
    // added in the review fixes (change log): checked NodeId -> handle conversions for the DSL
    // objects a test built, which call and probe records pass as {"$": NodeId} (PLAN §8.2;
    // V.matches filter tuples, add_lasting effects). None unless node(n).kind() is in the set.
    pub fn filter_id(&self, n: NodeId) -> Option<FilterId>;              // NodeKind::FILTER
    pub fn cond_id(&self, n: NodeId) -> Option<CondId>;                  // NodeKind::COND
    pub fn continuous_id(&self, n: NodeId) -> Option<ContinuousId>;      // NodeKind::CONTINUOUS
}
impl ContRef {
    pub fn repr(&self, reg: &Registry) -> String;                          // PLAN §4.7: StatMod(ap=.., hp=..), KeywordGrant(..), AbilityGrant(..)
    pub fn content_id(&self, reg: &Registry) -> ContentId;                 // runner/digest.py:26-27
}
impl ContentId {
    pub fn as_str(&self) -> &str;                                          // the 16 hex characters (ASCII by construction)
    pub fn from_hex(s: &str) -> Option<ContentId>;                         // 16 lowercase hex characters
}
impl gcg_data::ImplementationStatus for Registry;                          // cards[def].script is None (deck/validate.py:255-256)
impl fmt::Display for IrError;  impl std::error::Error for IrError;       // Version names IR_REGENERATE_COMMAND
```

Differences from PLAN §10.3.2 and §5.3, settled here:

- The DSL types are generated plain Rust (`gen_dsl.py`) rather than a `dsl_nodes!` macro; the
  macro's other outputs (decoder, `repr`, golden writer) are B1's functions over the generated
  types and the `CLASSES` table.
- `StepHook` has 13 variants, one per Python name: `wp_eb01_a_look_own_top` and
  `wp_eb01_b_look_top_own` stay separate (`EB01aLookOwnTop`, `EB01bLookTopOwn`) so that
  `Custom*.name` can be typed by the hook enum and `repr` stays exact; the two share one body
  in the engine (W-64).
- Typed hook parameters are `StepParams`/`CondParams`/`FilterParams`/`ValueParams`, decoded at
  load with `dict(params)` last-key-wins and returned by `Registry::hook_params`; the raw
  `params` stay in the node. `GD05aDestroyedByEffect { by_you }` is `params.get("by") == "you"`.
- `ContRef` lives in `gcg-effects` (PLAN §4.2 lists it under `gcg-engine`), because
  `Registry::cont_of` returns it and `gcg-effects` cannot depend on the engine; the engine
  re-exports it.
- `AbilityEntry.ability` is an `AbilityNodeId`, `rule_mod_repr` takes a `RuleModId`, and
  `Registry::{db, hook_params, filter_set}` and `ContentId::{as_str, from_hex}` are added.
- No prompt table (review fixes; O4): `PromptId`, `Registry.prompts` and `Registry::prompt` are
  dropped. PLAN §2.1 and §4.3 list one, but the engine serialises prompts from
  `gcg_engine::Prompt` (`EnginePrompt`, the `AskMay.prompt` `StrId`, the `Choose.var` `VarId`)
  and the IR schema (§3.3) has no prompt table to load it from.
- `Registry::{filter_id, cond_id, continuous_id}` are added (review fixes): `gcg-diff` and the
  engine's integration tests turn a test-built node's `NodeId` into the typed handle that
  `view::matches` and `testkit::engine::add_lasting` take.

## gcg-engine (owners C, B2) — PLAN §4, §10.3.3, §10.3.4, Inv 05, Inv 06a, Inv 06b

Types:

- `crates/gcg-engine/src/enums.rs`, generated by `port/oracle/gen_engine_enums.py`
  (`--write`, `--check` exits 1 when stale; lead-owned like `dsl.rs`):
  - the `IntEnum`s `Zone` (12), `Phase` (7) and `Step` (32), each discriminant the Python
    value (`engine/types.py:8-98`), with `{PY_CLASS, ALL, value, py_name, from_value}` and
    `Zone::{index, is_public, is_field}`;
  - the `StrEnum`s `DecisionKind` (13), `ActionKind` (18) and `EndReason` (5), declaration order
    as discriminants (`types.py:101-155`), with `{PY_CLASS, ALL, SORTED_BY_VALUE, as_str,
    py_name, from_value}`. `Duration` is `gcg_effects::Duration`, re-exported;
  - the literals an AST scan of `src/gcg_sim` finds at the call sites, sorted by value so the
    derived `Ord` is Python's string order, with `{ALL, as_str, from_value}`: `HistKind` (the 22
    `record` kinds, PLAN §4.2), `EvKey` (the 20 payload keys of `emit`), `FrameKind` (7),
    `EnginePrompt` (the 14 literal prompts, O4) and `CtxKey` (10). A call site the scan cannot
    read fails the generator, so a new literal in Python makes `--check` fail;
  - the zone sets `PUBLIC_ZONES`, `FIELD_ZONES`, `ORDERED_ZONES`, `TOKEN_ZONES`, `HIDDEN_ZONES`
    (value order) and `LISTED_ZONES` (iteration order), and the module constants `NO_ARG`,
    `PLAYER_TARGET`, `N_ZONES`, `BOTH_KNOW`, `HAND_LIMIT`, `BATTLE_LIMIT`, `BASE_LIMIT`,
    `RESOURCE_LIMIT`, `EX_RESOURCE_LIMIT`, `SHIELD_COUNT`, `START_HAND`, `REDUCE_ONCE_TAG`,
    `PREVENT_ONCE_TAG`, `REST_SUB_TAG`, `SUPPORT_AID`, `ALT_PLAY_BASE`, `MAX_ACTIONS_DEFAULT`,
    `MAIN_DECK_SIZE`, `RESOURCE_DECK_SIZE`, `MAX_COPIES`, `MAX_COLORS`, `BURST_PROMPT`,
    `HOLD_RULES` and (feature `testkit`) `FILLER_UNIT`, `FILLER_RESOURCE`, read from the
    Python modules;
  - the `to_json` key orders `STATE_JSON_KEYS` (34, `state.py:575-611`), `FRAME_JSON_KEYS`
    (16, `:372-390`) and `DECISION_JSON_KEYS` (5, `:51-58`), read from Python objects; `canon`
    writes exactly these orders.
- `crates/gcg-engine/src/contracts.rs`:
  - constants `DEFAULT_TURN_LIMIT`, `FIXPOINT_PASSES`, `NO_PROGRESS_STEPS` (W-47),
    `N_EVENTS`, `EVENT_CAPACITY`, `CONTEXT_CAPACITY`, `ONCE_KEY_CAPACITY`; `Uid`; re-exports
    `ContRef`, `Duration`;
  - records (frozen `pub` fields): `Action` (with `new`, `of`), `Span`, `CardInstance`,
    `Battle`, `AbilityKey`, `EventPayload`, `OnceElem`, `OnceKey`, `FpOnceElem`, `FpOnceKey`,
    `Lasting`, `TriggerInst`, `Frame`, `VarEntry`, `IntKey`, `IntEntry`, `HistoryEvent`,
    `HistKindRef` (landed: `decode`, `matches`), `Prompt`, `DecisionHdr`, `DecisionView`,
    `TestOverrides`, `Ctx`, `CtxVars`, `ViewKey`, `RuleInst`, `RuleKey`, `TraitsRef`, `Traits`,
    `Names`, `NewGameOptions` (Python defaults), `StateSnapshot`, and the errors
    `UnimplementedCardError`, `EngineError` (with `py_class`), `IllegalActionError`,
    `ApplyError`, `DeckConstructionError`, `NewGameError`, `CanonError`, `InvariantViolation`,
    `CheckError`; feature `testkit`: `ScenarioOptions`, `AddOptions`, `LastingArgs` (Python
    defaults), `FrameArgs`, `TestkitError`;
  - the shared state `GameState`: `pub(crate)` fields frozen like a record (C and B2 both use
    them; arenas change only through the L1 helpers below);
  - the internal-boundary types `UidBuf`, `MoveOpts`, `DamageOpts`, `EngineVars` (C's) and
    `PushFrame`, `LastingOpts` (B2's): frozen, because the other owner uses them;
  - opaque types (`pub(crate)` fields are the named owner's starting layout): `Derived`,
    `TriggerIndex` (B2); `ZoneLists`, `GameConst`, `FrameVars`, `Lki`,
    `LkiEntry`, `CardMultiset`, `InfoSetKey`, `InfoCard`, `Scenario` (C).

Every record documents its Python `to_json` order on the type (`engine/state.py:23-24`,
`:51-58`, `:118-130`, `:173-185`, `:231-245`, `:294-306`, `:372-390`, `:425-426`, `:575-611`);
`canon` writes exactly those orders.

### Layers, modules and owners (PLAN §10.1)

| Layer | Owner | Modules (files) | Python | Gate |
| --- | --- | --- | --- | --- |
| L1 | C | `state` (`state.rs`), `zones` (`zones.rs`), `canon` (`canon.rs`, public), `error` (`error.rs`) | `engine/state.py`, `runner/digest.py` | M5.1 |
| L2 | B2 | `view` (`view/**`, public) | `engine/view.py`, the 21 cond/filter/value hooks | M5.2 |
| L3 | C | `core` (`core.rs`), `battle` (`battle.rs`) | `engine/core.py`, `engine/battle.py` | M5.3 |
| L4 | B2 | `interp` (`interp/**`) | `engine/interp.py`, `effects/builtins.py`, the 13 step hooks | M5.4 |
| L5 | C | `game` (`game.rs`), `observe` (`observe.rs`, public), `invariants` (`invariants.rs`, public), `procedures` (`procedures.rs`, public), `testkit` (`testkit/**`, public, feature `testkit`), `crates/gcg-engine/tests/**` | `engine/game.py`, `observe.py`, `invariants.py`, `testkit.py` | M5.5 |

`lib.rs` is lead-owned: each landing report names the `mod` lines it needs, and the lead adds
them at integration. The crate root re-exports `game::{new_game, new_game_by_number, apply,
legal_actions, concede, step, deck_construction_problems, deck_construction_problems_by_number}`. `types.rs` of PLAN §10.1 L1 is not needed: the enums are
generated into `enums.rs`. `crates/gcg-engine/Cargo.toml` has a self dev-dependency with
`testkit`, so C's `tests/**` see the testkit.

### Public API (consumed by D, E, F, G)

```rust
// L1 (C) — state.rs, zones.rs: read-only accessors (engine/state.py:433-651)
impl GameState {
    pub fn pending(&self) -> Option<DecisionView<'_>>;        // st.pending (header + options)
    pub fn cards(&self) -> &[CardInstance];                    // st.cards, index = uid
    pub fn card(&self, uid: Uid) -> &CardInstance;             // st.cards[uid]; uid < cards().len() (panics otherwise: an engine bug)
    pub fn def_of(&self, uid: Uid) -> &'static CardDef;        // view.py:23-24 cdef
    pub fn zone(&self, p: u8, z: Zone) -> &[Uid];              // state.py:514-515
    pub fn seed(&self) -> u64;                                 // st.seed
    pub fn rng_state(&self) -> u64;                            // st.rng.state
    pub fn turn(&self) -> i32;
    pub fn active(&self) -> u8;
    pub fn standby(&self) -> u8;                               // :517-519
    pub fn first_player(&self) -> u8;
    pub fn setup_chooser(&self) -> u8;
    pub fn phase(&self) -> Phase;
    pub fn step(&self) -> Step;
    pub fn priority(&self) -> u8;
    pub fn passes(&self) -> i32;
    pub fn battle(&self) -> Option<&Battle>;                   // :521-523
    pub fn battles(&self) -> &[Battle];
    pub fn frames(&self) -> &[Frame];
    pub fn frame_vars(&self, f: &Frame) -> FrameVars<'_>;
    pub fn frame_buf(&self, f: &Frame) -> &[i32];
    pub fn frame_ints(&self, f: &Frame) -> &[IntEntry];
    pub fn pending_triggers(&self) -> &[TriggerInst];
    pub fn batches(&self) -> impl Iterator<Item = &[TriggerInst]> + '_;   // st.batches, index 0 first
    pub fn lasting(&self) -> &[Lasting];
    pub fn delayed(&self) -> &[Lasting];
    pub fn lasting_targets(&self, l: &Lasting) -> &[(Uid, i32)];
    pub fn once_used(&self) -> &[OnceKey];                     // insertion order of a Python set
    pub fn history(&self) -> &[HistoryEvent];
    pub fn history_epoch(&self) -> u32;                        // runner/instrument.py:81 identity
    pub fn winner(&self) -> Option<i8>;
    pub fn game_over(&self) -> bool;                           // :525-527
    pub fn end_reason(&self) -> Option<EndReason>;
    pub fn decklists(&self) -> [&[DefId]; 2];
    pub fn resource_decklists(&self) -> [&[DefId]; 2];
    pub fn redraws(&self) -> [bool; 2];
    pub fn version(&self) -> u64;
    pub fn action_count(&self) -> i64;
    pub fn event_group(&self) -> i64;
    pub fn turn_limit(&self) -> i32;
    pub fn max_actions(&self) -> i64;
    pub fn rules_checked(&self) -> i64;                        // Python _rules_checked
    pub fn registry(&self) -> &Arc<Registry>;
    pub fn db(&self) -> &'static CardDb;
    pub fn clone_from_state(&mut self, src: &GameState);      // PLAN §4.5: Clone semantics into recycled buffers
}
impl EventPayload { pub fn get(&self, key: EvKey) -> Option<i32>; }          // first match (_payload core.py:187-191, Ctx.ev view.py:46-50)
impl DecisionHdr { pub fn ctx(&self, key: CtxKey) -> i32; }                   // state.py:45-49 (default NO_ARG)
impl Prompt { pub fn text<'r>(&self, reg: &'r Registry) -> Cow<'r, str>; }   // the serialised prompt (state.py:56)
impl<'a> FrameVars<'a> {
    pub fn get(&self, var: VarId) -> Option<&'a [i32]>;                       // f.vars.get(var)
    pub fn iter(&self) -> impl Iterator<Item = (VarId, &'a [i32])> + 'a;     // f.vars.items(), order unspecified (ai/actions.py:292 uses any())
}
impl OnceKey {
    pub fn contains_int(&self, x: i32) -> bool;               // `x in key` (ai/actions.py:309); a Cont element never equals x (§8.9)
    pub fn with_self(&self, uid: i32) -> FpOnceKey;           // tuple(_SELF if x == uid else x for x in key) (:309)
}
impl fmt::Display for Action;                                 // state.py:30-32: drops every -1 argument by value
impl Action { pub fn to_json(&self) -> Json; }                // state.py:23-24
// error.rs (C): Display = Python's message, std::error::Error, and From conversions
impl fmt::Display for EngineError; impl std::error::Error for EngineError;
impl fmt::Display for IllegalActionError; impl fmt::Display for ApplyError;          // game.py:783-786
impl fmt::Display for DeckConstructionError; impl fmt::Display for NewGameError;     // game.py:127
impl fmt::Display for UnimplementedCardError;                                         // registry.py:146
impl fmt::Display for CanonError; impl fmt::Display for InvariantViolation;           // invariants.py:108-109
impl fmt::Display for CheckError;
impl From<EngineError> for ApplyError; impl From<IllegalActionError> for ApplyError;
impl From<EngineError> for NewGameError; impl From<EngineError> for CheckError;
// (each with impl std::error::Error)

pub mod canon {                                               // C, L1; runner/digest.py, PLAN §4.7-§4.8
    pub fn canonical_state(st: &GameState) -> Json;           // digest.py:36-49 (34 keys, state.py:575-611 order)
    pub fn state_digest(st: &GameState) -> String;            // digest.py:52-56 (sort_keys, (",", ":"), ensure_ascii)
    pub fn from_canonical(reg: &Arc<Registry>, snap: &StateSnapshot) -> Result<GameState, CanonError>;   // PLAN §4.8
}

// L2 (B2) — view/ (engine/view.py)
pub mod view {
    pub fn derived(st: &GameState) -> Result<Arc<Derived>, EngineError>;          // view.py:232-239 (fills st's cache, PLAN §4.6)
    pub fn ap_of(st: &GameState, dv: &Derived, uid: Uid) -> i32;                 // :490-494
    pub fn hp_of(st: &GameState, dv: &Derived, uid: Uid) -> i32;                 // :497-504
    pub fn level_of(st: &GameState, uid: Uid) -> i32;                            // :507-509
    pub fn cost_of(st: &GameState, uid: Uid) -> i32;                             // :512-514
    pub fn play_cost(st: &GameState, dv: &Derived, uid: Uid) -> Result<i32, EngineError>;   // :517-518 (player_cost_mod calls derived(st), :525-545)
    pub fn play_level(st: &GameState, dv: &Derived, uid: Uid) -> i32;            // :521-522
    pub fn keywords(dv: &Derived, uid: Uid) -> KwAmounts;                        // :548-549
    pub fn has_kw(dv: &Derived, uid: Uid, kw: Kw) -> bool;                       // :552-553
    pub fn kw_amount(dv: &Derived, uid: Uid, kw: Kw) -> i32;                     // :556-557
    pub fn traits_of<'a>(st: &'a GameState, dv: &'a Derived, uid: Uid) -> Traits<'a>;       // :560-562
    pub fn rules_of<'a>(dv: &'a Derived, uid: Uid, kind: RuleKind) -> impl Iterator<Item = &'a RuleInst> + 'a;   // :565-566
    pub fn rules_all(dv: &Derived, uid: Uid) -> &[RuleInst];                    // dv.rules.get(uid, ()) (ai/actions.py:360)
    pub fn is_linked(dv: &Derived, uid: Uid) -> bool;                            // :569-570
    pub fn abilities_of(dv: &Derived, uid: Uid) -> &[AbilityId];                 // dv.abilities.get(uid, ()) (game.py:415, ai/actions.py:358)
    pub fn trigger_index<'a>(st: &GameState, dv: &'a Derived) -> &'a TriggerIndex;          // :314-333 (fills dv.by_event once)
    pub fn link_satisfied(st: &GameState, unit: &CardDef, pilot: &CardDef) -> bool;         // :125-138 (st only for TestOverrides::force_link)
    pub fn link_satisfied_unpatched(reg: &Registry, unit: &CardDef, pilot: &CardDef) -> bool;   // :125-138 without TestOverrides (step 3: ai/setup.py:18 binds the unpatched function)
    pub fn names_of<'a>(reg: &'a Registry, cd: &'a CardDef) -> Names<'a>;                   // :471-474
    pub fn matches(st: &GameState, dv: &Derived, ctx: &Ctx, uid: Uid, filters: &[FilterId]) -> Result<bool, EngineError>;   // :702-705
    pub fn stat(st: &GameState, dv: &Derived, uid: Uid, s: Stat) -> i32;         // :673-685
    pub fn same_values(a: &Derived, b: &Derived) -> bool;                        // :94-106 (AI trial worlds, ai/actions.py:230)
    pub fn signature(st: &GameState, dv: &Derived) -> Json;                      // tests/paranoid.py:24-39 view_signature
}
impl Derived {
    pub fn empty() -> Derived;                                // _UNREAD = Derived() (view.py:845)
    pub fn key(&self) -> ViewKey;
    pub fn converged(&self) -> bool;                          // W-48 diagnostic (fuzz and paranoid read it)
}
impl TriggerIndex { pub fn get(&self, ev: Ev) -> &[(Uid, AbilityId)]; }       // idx.get(ev, ())
impl<'a> Traits<'a> { pub fn iter(&self, reg: &'a Registry) -> impl Iterator<Item = &'a str> + 'a; }
impl<'a> Names<'a> { pub fn iter(&self, reg: &'a Registry) -> impl Iterator<Item = &'a str> + 'a; }
impl Ctx {
    pub fn new(controller: u8, host: i32, card_uid: i32, vars: CtxVars, event: EventPayload) -> Ctx;   // view.py:32-44 (card_uid -1 -> host)
    pub fn ev(&self, key: EvKey, default: i32) -> i32;        // view.py:46-50
}

// L3 + L5 (C) — game.rs, battle.rs, procedures.rs (engine/game.py, engine/battle.py)
pub fn new_game(reg: &Arc<Registry>, decks: [&DeckList; 2], seed: u64, opts: NewGameOptions)
    -> Result<GameState, NewGameError>;                       // game.py:107-158
pub fn deck_construction_problems(db: &CardDb, deck: &DeckList) -> Vec<String>;   // game.py:58-100
// Python's DeckList holds card numbers as written (game.py:41-45). Replay documents
// (runner/replayfile.py:68-72) and trace headers (gcg_diff::TraceDeck) carry such strings,
// which may be unknown, product ids or padded; these two take them (review fixes, change log).
/// game.py:58-100 verbatim on the strings: the size problems, then per key `normalize_id(n) or n`
/// in sorted order `unknown card {number}` or the type and copy problems, then the colours,
/// then per sorted(set(resources)) the raw text. deck_construction_problems(db, deck) equals
/// this on the card numbers of deck's defs.
pub fn deck_construction_problems_by_number(db: &CardDb, main: &[&str], resources: &[&str]) -> Vec<String>;
/// game.py:107-158 on Python's string DeckList: with opts.validate, the problems above per
/// player (DeckConstructionError, player 0 first) before any lookup; then, per player in
/// Python's order, `R.db[n]` for each main number in caller order (CardDb::get: strip, product
/// ids; unknown is EngineError::Key with Python's KeyError text, reachable only without
/// validate), R.require, the resource numbers; then exactly new_game.
pub fn new_game_by_number(reg: &Arc<Registry>, decks: [(&[&str], &[&str]); 2], seed: u64,
                          opts: NewGameOptions) -> Result<GameState, NewGameError>;
pub fn apply(st: &mut GameState, action: Action, check: bool) -> Result<(), ApplyError>;   // game.py:780-818
pub fn legal_actions(st: &GameState) -> &[Action];           // game.py:774-777 (empty without a decision)
pub fn concede(st: &mut GameState, player: u8);              // game.py:763-771
pub fn step(st: &GameState, action: Action) -> Result<GameState, ApplyError>;   // game.py:961-965
pub mod procedures {     // engine entry points the AI calls on its own clones (read-only ones may fill the view cache)
    pub fn declare_attack(st: &mut GameState, attacker: Uid, target: i32) -> Result<(), EngineError>;   // battle.py:12-32
    pub fn eligible_blockers(st: &GameState, b: &Battle) -> Result<Vec<Uid>, EngineError>;              // battle.py:47-73
    pub fn main_options(st: &GameState) -> Result<Vec<Action>, EngineError>;                           // game.py:543-583
    pub fn action_step_options(st: &GameState, p: u8) -> Result<Vec<Action>, EngineError>;             // game.py:647-660
    pub fn activated_list(st: &GameState, p: u8, timing: Timing) -> Result<Vec<(Uid, i32, i32)>, EngineError>;   // game.py:410-433 (host, aid or SUPPORT_AID, cost)
    pub fn command_playable(st: &GameState, p: u8, uid: Uid, timing: Timing) -> Result<bool, EngineError>;       // game.py:395-407
    pub fn set_rested_for_trial(st: &mut GameState, uid: Uid, rested: bool);                            // ai/actions.py:228-229 (field write + touch)
}
pub mod observe {        // C, L5 (engine/observe.py); re-exports crate::HIDDEN_ZONES
    pub fn is_hidden_from(st: &GameState, uid: Uid, observer: u8) -> bool;                      // :22-24
    pub fn unknown_instances(st: &GameState, observer: u8, owner: u8) -> Vec<Uid>;             // :27-36 (uid order)
    pub fn unknown_pools(st: &GameState, observer: u8, owner: u8) -> (Vec<DefId>, Vec<DefId>); // :39-53 (sorted)
    pub fn determinize(st: &GameState, observer: u8, seed: u64) -> Result<GameState, EngineError>;          // :56-83 (_fit ValueError :98-110)
    pub fn determinize_into(st: &GameState, observer: u8, seed: u64, out: &mut GameState) -> Result<(), EngineError>;   // the same into a recycled state (PLAN §4.5)
    pub fn permute_hidden(st: &GameState, observer: u8, seed: u64) -> GameState;              // :113-131
    pub fn information_set_key(st: &GameState, observer: u8) -> InfoSetKey;                   // :134-150
}
pub mod invariants {     // C, L5 (engine/invariants.py); re-exports crate::LISTED_ZONES
    pub fn check(st: &GameState, initial: Option<&CardMultiset>) -> Result<(), CheckError>;   // :29-109
    pub fn initial_multiset(st: &GameState) -> CardMultiset;                                   // :112-116
}

#[cfg(feature = "testkit")]
pub mod testkit {        // C, L5 (testkit.py); helpers return Python's AssertionError texts as TestkitError::Assertion
    impl Scenario {
        pub fn new(reg: &Arc<Registry>, opts: ScenarioOptions) -> Scenario;                     // :38-49
        pub fn add(&mut self, player: u8, number: &str, zone: Zone, opts: AddOptions<'_>) -> Result<Uid, TestkitError>;   // :52-82 (R.require)
        pub fn add_raw(&mut self, player: u8, number: &str, zone: Zone) -> Result<Uid, TestkitError>;                    // :84-91
        pub fn hand(&mut self, player: u8, numbers: &[&str]) -> Result<Vec<Uid>, TestkitError>;       // :93-94
        pub fn trash(&mut self, player: u8, numbers: &[&str]) -> Result<Vec<Uid>, TestkitError>;      // :96-97
        pub fn shields(&mut self, player: u8, numbers: &[&str]) -> Result<Vec<Uid>, TestkitError>;    // :99-106 (first = top)
        pub fn deck(&mut self, player: u8, numbers: &[&str]);                                         // :108-111 (first = top)
        pub fn resources(&mut self, player: u8, n: usize, rested: usize, ex: usize) -> Result<Vec<Uid>, TestkitError>;   // :113-119
        pub fn base(&mut self, player: u8, number: Option<&str>, damage: i32) -> Result<Uid, TestkitError>;               // :121-123
        pub fn resource_deck(&mut self, player: u8, n: usize) -> Result<(), TestkitError>;           // :125-127
        pub fn force_link(&mut self, unit_numbers: &[&str]);                                          // tests/cards/test_wp_gd05_a.py:101-113
        pub fn start(&mut self, step: Step) -> Result<(), TestkitError>;                              // :160-182 (+ _build_decks :130-142, _record_decklists :144-158)
        pub fn state(&self) -> &GameState;                                                            // sc.st
        pub fn state_mut(&mut self) -> &mut GameState;
        pub fn into_state(self) -> GameState;
    }
    pub fn vanilla_deck(db: &CardDb, color: &str, resource: &str) -> Result<DeckList, TestkitError>;   // :185-199
    pub fn act(st: &mut GameState, kind: ActionKind, a: Option<i32>, b: Option<i32>, c: Option<i32>) -> Result<Action, TestkitError>;   // :237-254
    pub fn has_action(st: &GameState, kind: ActionKind, a: Option<i32>, b: Option<i32>) -> bool;  // :257-260
    pub fn play(st: &mut GameState, uid: Uid, onto: Option<Uid>, ex: i32) -> Result<Action, TestkitError>;   // :263-272
    pub fn activate(st: &mut GameState, host: Uid, ability: Option<i32>, ex: i32) -> Result<Action, TestkitError>;   // :275-276
    pub fn attack(st: &mut GameState, attacker: Uid, target: i32) -> Result<Action, TestkitError>;  // :279-280 (Python default PLAYER_TARGET)
    pub fn end_main(st: &mut GameState) -> Result<Action, TestkitError>;                          // :283-284
    pub fn block(st: &mut GameState, blocker: Option<Uid>) -> Result<Action, TestkitError>;       // :287-290
    pub fn pass(st: &mut GameState) -> Result<Action, TestkitError>;                              // :293-294 pass_
    pub fn yes(st: &mut GameState) -> Result<Action, TestkitError>;                               // :297-298
    pub fn no(st: &mut GameState) -> Result<Action, TestkitError>;                                // :301-302
    pub fn yes_if_asked(st: &mut GameState) -> Result<(), TestkitError>;                          // :305-308
    pub fn select(st: &mut GameState, uids: &[Uid], done: Option<bool>) -> Result<(), TestkitError>;   // :311-322
    pub fn arrange_if_asked(st: &mut GameState) -> Result<(), TestkitError>;                      // :325-329
    pub fn select_if_asked(st: &mut GameState, uids: &[Uid], done: Option<bool>) -> Result<(), TestkitError>;   // :332-337
    pub fn choose_option(st: &mut GameState, index: i32) -> Result<Action, TestkitError>;         // :340-341
    pub fn order(st: &mut GameState, index: usize) -> Result<Action, TestkitError>;               // :344-345 (Python default 0)
    pub fn pass_all(st: &mut GameState, max_steps: usize) -> Result<(), TestkitError>;            // :348-360 (Python default 50)
    pub fn to_next_turn(st: &mut GameState) -> Result<(), TestkitError>;                          // :363-380
    pub fn zone_of(st: &GameState, uid: Uid) -> Zone;                                             // :383-384
    pub fn ap(st: &GameState, uid: Uid) -> Result<i32, TestkitError>;                             // :387-388
    pub fn hp(st: &GameState, uid: Uid) -> Result<i32, TestkitError>;                             // :391-392
    pub fn keywords(st: &GameState, uid: Uid) -> Result<Vec<(Kw, i32)>, TestkitError>;           // :395-396 (Kw declaration order)
    pub fn uids_in(st: &GameState, player: u8, zone: Zone) -> Vec<Uid>;                          // :399-400
    pub fn numbers_in(st: &GameState, player: u8, zone: Zone) -> Vec<&'static str>;              // :403-404
    pub fn legal_kinds(st: &GameState) -> Vec<ActionKind>;                                        // :407-408 (a set: deduplicated, ActionKind order)
    pub fn expect_illegal(st: &mut GameState, action: Action) -> Result<(), TestkitError>;       // :411-416
    pub fn run_random(st: &mut GameState, seed: u64, max_actions: usize) -> Result<(), TestkitError>;   // :419-428 (Python default 5000)
    pub fn names(st: &GameState, uids: &[Uid]) -> Vec<&'static str>;                             // :431-432
    pub fn card_numbers(st: &GameState, uids: &[Uid]) -> Vec<&'static str>;                      // :435-436

    // Trace replay hooks (review fixes; gcg-diff's EngineUnderTest, PLAN §8.2 per-test records)
    /// A "state" record with "derived":"stale": computes view::derived(view_from), fails with
    /// EngineError::Internal unless its key is `key`, and stores it as st's cached view, so the
    /// next view::derived(st) returns it while st's key equals `key` (view.py:232-239). No touch.
    pub fn install_view(st: &GameState, view_from: &GameState, key: ViewKey) -> Result<(), EngineError>;
    /// A "force_link" record: st's GameConst gets TestOverrides { force_link: units } (a new
    /// Arc; clones share it from here on). No touch: the monkeypatch of
    /// tests/cards/test_wp_gd05_a.py:101-113 touches nothing.
    pub fn set_force_link(st: &mut GameState, units: &[&str]);

    /// Direct calls of Python tests into engine internals ("call" records, PLAN §8.2), which
    /// gcg-diff's dispatcher and C's tests/** reach only through these. Each forwards to the
    /// internal function with Python's parameters (C lands the module; the B2 ones forward to
    /// the boundary names below). Census: an AST scan of tests/ for calls through engine
    /// module aliases and imported names.
    pub mod engine {
        pub fn push_frame(st: &mut GameState, program: ProgramId, args: FrameArgs<'_>)
            -> Result<usize, EngineError>;                     // interp.py:61-88 (the new frame's index); tests/rules/test_battle.py:88, test_cardinfo.py:113, test_damage.py:120, test_zones.py:186
        pub fn advance(st: &mut GameState) -> Result<(), EngineError>;   // game.py:734-760; tests/rules/test_battle.py:103, test_cardinfo.py:114, tests/cards/test_wp_eb01_b.py:87, test_wp_gd05_a.py:451, :1211
        pub fn add_lasting(st: &mut GameState, effect: ContinuousId, controller: u8, source: i32,
                           targets: &[(Uid, i32)], duration: Duration, o: LastingArgs<'_>)
            -> Result<(), EngineError>;                        // interp.py:1286-1320 (effect via Registry::cont_of; a filter tuple outside the registry's filter sets is EngineError::Internal); tests/rules/test_keywords.py:119, tests/cards/test_wp_gd05_b.py:1938
        pub fn draw(st: &mut GameState, p: u8, n: i64, by_effect: bool) -> Result<i64, EngineError>;   // interp.py:500-512; tests/rules/test_turn.py:487
        pub fn card_type(st: &GameState, uid: Uid) -> CardType;                                        // core.py:945-946; tests/rules/test_faq.py:120
        pub fn deploy_cards(st: &mut GameState, cards: &[Uid], rested: bool, by: i32, ex_used: i32)
            -> Result<(), EngineError>;                        // interp.py:956-976 (Python default ex_used=0); tests/cards/test_wp_gd05_a.py:1209
    }
}
```

The census also finds `core.clear_listen_cache` (`tests/ai/test_root_moves.py:368`, `:373`,
and `testkit.py:212`, `:217` inside `card_text`), which has no Rust function: `GameConst.listened` is computed per game
from its own registry, and every override test builds its states inside the override, so
Python's cleared cache and Rust's per-game set give the same events. G's dispatcher maps the
name to a no-op whose result is `None`; the record's `after16` still checks that nothing
changed. Every other name it finds (`game.{apply, new_game, legal_actions, concede}`,
`observe.*`, `invariants.{check, initial_multiset}`) is public API already.

`options` (`testkit.py:224-225`) is `legal_actions`. `card_text` (`:203-217`) has no Rust
function: Rust cannot compile card text, so a test builds its scenario on
`reg.with_fragment(fragment)` with the IR fragment Python exported for it (PLAN §3.4), and
`core.clear_listen_cache` has no counterpart because `GameConst.listened` belongs to one game.

### Internal layer boundary (`pub(crate)`, PLAN §10.3.4)

Exactly the names one owner's layer calls in the other's, with Python's parameter order;
keyword-only arguments are option structs, and every function that can raise in Python
returns `Result<_, EngineError>`. Names used only inside one owner's layers (for example
`resolve_damage`, `queue_trigger`, `advance`, `player_of`) are that owner's choice.

```rust
// L1 (C), state.rs: the arena helpers B2 uses; scalar fields and cards/battles are written directly
impl GameState {
    pub(crate) fn touch(&mut self);                                          // state.py:529-531: version += 1; cache.get_mut() = None
    pub(crate) fn once_add(&mut self, key: OnceKey);                         // st.once_used.add (no duplicate, PLAN §4.3)
    pub(crate) fn once_contains(&self, key: &OnceKey) -> bool;               // `key in st.once_used`
    pub(crate) fn set_pending(&mut self, hdr: DecisionHdr, options: &[Action]);   // st.pending = Decision(...) (interp.py:101)
    pub(crate) fn frame_push(&mut self, f: Frame, vars: &[(VarId, &[i32])]) -> Result<usize, EngineError>;   // st.frames.append (interp.py:87); f's spans are ignored
    pub(crate) fn frame_pop(&mut self) -> Option<Frame>;                     // st.frames.pop() (interp.py:131); truncates the arenas
    pub(crate) fn frame_mut(&mut self, i: usize) -> &mut Frame;              // f.pc, f.did, f.waiting, ... (never the spans)
    pub(crate) fn var_get(&self, frame: usize, var: VarId) -> Option<&[i32]>;               // f.vars.get(var)
    pub(crate) fn var_set(&mut self, frame: usize, var: VarId, items: &[i32]) -> Result<(), EngineError>;   // f.vars[var] = tuple(items)
    pub(crate) fn var_del(&mut self, frame: usize, var: VarId);                              // del f.vars[var] (interp.py:828)
    pub(crate) fn int_get(&self, frame: usize, key: IntKey) -> Option<i32>;                  // f.ints.get(key)
    pub(crate) fn int_set(&mut self, frame: usize, key: IntKey, value: i32) -> Result<(), EngineError>;   // f.ints[key] = value
    pub(crate) fn int_del(&mut self, frame: usize, key: IntKey) -> Option<i32>;              // del / f.ints.pop(key, None)
    pub(crate) fn buf(&self, frame: usize) -> &[i32];                                        // f.buf
    pub(crate) fn buf_set(&mut self, frame: usize, items: &[i32]) -> Result<(), EngineError>;   // f.buf = [...] / [] (interp.py:414, :1168, ...)
    pub(crate) fn buf_push(&mut self, frame: usize, x: i32) -> Result<(), EngineError>;      // f.buf.append(x) (:454, :1215)
    pub(crate) fn lasting_push(&mut self, l: Lasting, targets: &[(Uid, i32)]) -> Result<(), EngineError>;   // st.lasting.append (interp.py:1303); l.targets ignored
    pub(crate) fn delayed_push(&mut self, l: Lasting, targets: &[(Uid, i32)]) -> Result<(), EngineError>;   // st.delayed.append (interp.py:1557)
    pub(crate) fn engine_vars(&self) -> &EngineVars;                        // VarIds of card, pilot, unit, __todeck, __play, looked, t
    pub(crate) fn test_overrides(&self) -> Option<&TestOverrides>;           // force_link for view::link_satisfied
    // Zone-list edits without a zone change (review fixes): B2 reorders DECK in place at
    // interp.py:1194-1202 (_place_ordered) and builtins.py:20-26 (return_looked_bottom). None of
    // them touches, bumps zone_seq, records, emits or changes `known`; the caller touches, as
    // Python does.
    pub(crate) fn zone_remove(&mut self, p: u8, z: Zone, uid: Uid) -> Result<(), EngineError>;   // list.remove(u): the first occurrence; absent is EngineError::Value("list.remove(x): x not in list")
    pub(crate) fn zone_extend(&mut self, p: u8, z: Zone, uids: &[Uid]) -> Result<(), EngineError>;   // list.extend(uids) (Overflow past the arena's u16 offsets)
    pub(crate) fn zone_insert_front(&mut self, p: u8, z: Zone, uids: &[Uid]) -> Result<(), EngineError>;   // lst[0:0] = uids (the same)
}
// L2 (B2), view/: called by C's core, battle, game, observe, invariants
pub(crate) fn value(st: &GameState, dv: &Derived, ctx: &Ctx, v: ValueRef) -> Result<i64, EngineError>;          // view.py:853-882
pub(crate) fn cond(st: &GameState, dv: &Derived, ctx: &Ctx, c: CondId, did: bool) -> Result<bool, EngineError>;  // :885-933
pub(crate) fn resolve(st: &GameState, dv: &Derived, ctx: &Ctx, r: RefId) -> Result<UidBuf, EngineError>;        // :784-831
pub(crate) fn resolve_ref(st: &GameState, ctx: &Ctx, r: RefId) -> Result<UidBuf, EngineError>;                  // :848-850 (derived only if the ref selects, :834-842)
pub(crate) fn select(st: &GameState, dv: &Derived, ctx: &Ctx, sel: SelId) -> Result<UidBuf, EngineError>;       // :626-632
pub(crate) fn player_cost_mod(st: &GameState, uid: Uid) -> Result<i64, EngineError>;                            // :525-545
pub(crate) fn zone_loc_code(z: Zone) -> i32;                                                                     // :952-954
// L3 (C), core.rs and battle.rs: called by B2's interp and hooks
pub(crate) fn move_card(st: &mut GameState, uid: Uid, dst: Zone, o: MoveOpts) -> Result<(), EngineError>;   // core.py:55-81 `move`
pub(crate) fn remove_from_game(st: &mut GameState, uid: Uid) -> Result<(), EngineError>;                    // :109-116
pub(crate) fn new_card(st: &mut GameState, def: DefId, owner: u8, zone: Zone, rested: bool) -> Result<Uid, EngineError>;   // :119-131 (Overflow past u16::MAX cards)
pub(crate) fn shuffle_deck(st: &mut GameState, p: u8);                                                     // :134-139
pub(crate) fn record(st: &mut GameState, kind: HistKind, player: u8, by: i8, uid: i32, source: i32);       // :142-151 (no touch)
pub(crate) fn next_group(st: &mut GameState) -> i64;                                                       // :158-160
pub(crate) fn emit(st: &mut GameState, ev: Ev, subject: i32, player: i32, lki: Option<&Lki>,
                   extra: &[(EvKey, i32)]) -> Result<(), EngineError>;                                     // :316-356 (extra may carry Group; sorted here)
pub(crate) fn ability_snapshot(st: &GameState, uids: &[Uid]) -> Result<Lki, EngineError>;                 // :494-496 (calls derived)
pub(crate) fn card_type(st: &GameState, uid: Uid) -> CardType;                                             // :945-946
pub(crate) fn damage_card(st: &mut GameState, uid: Uid, amount: i64, d: DamageOpts) -> Result<i64, EngineError>;    // :570-573
pub(crate) fn apply_damage(st: &mut GameState, uid: Uid, amount: i64, d: DamageOpts) -> Result<i64, EngineError>;   // :602-633
pub(crate) fn shields_receiving_damage(st: &mut GameState, uids: &[Uid], amount: i64, d: DamageOpts) -> Result<UidBuf, EngineError>;   // :681-697 (once keys)
pub(crate) fn destroy_shields(st: &mut GameState, p: u8, uids: &[Uid], d: DamageOpts) -> Result<(), EngineError>;  // :700-749
pub(crate) fn damage_shield_area(st: &mut GameState, p: u8, amount: i64, cards: usize, d: DamageOpts) -> Result<(), EngineError>;   // :752-767
pub(crate) fn destroy(st: &mut GameState, uids: &[Uid], battle: bool, by: i32, source: i32) -> Result<UidBuf, EngineError>;          // :774-827
pub(crate) fn set_winner(st: &mut GameState, losers: &[u8], reason: EndReason);                            // :842-854 (losers: a set of 1 or 2 players)
pub(crate) fn rules_management(st: &mut GameState) -> Result<(), EngineError>;                             // :857-892
pub(crate) fn over(st: &GameState) -> bool;                                                                // :940-942
pub(crate) fn damage_only_battle(st: &mut GameState, attacker: Uid, target: Uid) -> Result<(), EngineError>;  // battle.py:261-282 (touches :277, :282)
// L4 (B2), interp/: called by C's game and testkit::engine
pub(crate) fn add_lasting(st: &mut GameState, effect: ContRef, controller: u8, source: i32, targets: &[(Uid, i32)],
                          duration: Duration, o: LastingOpts) -> Result<(), EngineError>;   // interp.py:1286-1320 (effect is R.cont_key(effect), the canonical ContRef)
pub(crate) fn deploy_cards(st: &mut GameState, cards: &[Uid], rested: bool, by: i32, ex_used: i32) -> Result<(), EngineError>;   // :956-976
pub(crate) fn push_frame(st: &mut GameState, program: ProgramId, f: PushFrame<'_>) -> Result<usize, EngineError>;   // interp.py:61-88
pub(crate) fn run_top_frame(st: &mut GameState) -> Result<(), EngineError>;                                // :109-132
pub(crate) fn resume(st: &mut GameState, action: Action) -> Result<(), EngineError>;                       // :160-175
pub(crate) fn draw(st: &mut GameState, p: u8, n: i64, by_effect: bool) -> Result<i64, EngineError>;        // :500-512 (cards drawn)
pub(crate) fn place_resource(st: &mut GameState, p: u8, rested: bool, by: i32) -> Result<bool, EngineError>;   // :1051-1059
pub(crate) fn pay_generic(st: &mut GameState, p: u8, cost: i64, ex_used: i32) -> Result<bool, EngineError>;    // :1442-1470 (ex_used -1 = prefer normal)
pub(crate) fn can_pair(st: &GameState, dv: &Derived, unit: Uid) -> bool;                                  // :1340-1344
pub(crate) fn targets_available(st: &GameState, program: ProgramId, start: u16, ctx: &Ctx) -> Result<bool, EngineError>;   // :235-260 (Python passes the program's instrs)
pub(crate) fn system_program(st: &GameState, s: SystemProgram) -> ProgramId;                              // :1589-1594 (fixed IR ids)
```

Behaviour fixed by the contract (PLAN §4.4-§4.6):

- `touch` is called exactly where Python calls `st.touch()` (Inv 05 §3.2, Inv 06a §4,
  Inv 06b §15), never elsewhere; `core.record` does not touch.
- `view::derived` follows `view.py:232-239` in the four steps of PLAN §4.6: clone the cached
  `Arc` out of the `RefCell` and release the borrow, compute `ViewKey` before any compute,
  recompute with no borrow held when the cache is empty or the key differs, then store and
  return. `touch` is the only invalidation.
- Feature `paranoid` (W-49, W-124; port of `tests/paranoid.py:41-132`): every cache hit of
  `derived` is recomputed and compared (`view::signature`, and the trigger index when filled),
  every rules-management skip (`core.py:861`, `:868-876`) re-runs the scan on a clone and
  requires no change, every unlistened event (`core.py:334-335`) is shown to have no
  candidate, and every view-free `resolve_ref` resolves the same with the view. A mismatch
  panics with the check's name: these are gate diagnostics, never in a shipped build.
- W-48: a view that reaches `FIXPOINT_PASSES` without converging keeps the last pass (as
  Python) and reports `Derived::converged() == false`.
- W-47: `advance` returns `EngineError::NoProgress` after `NO_PROGRESS_STEPS` loop iterations
  without a decision. W-143: `resolve_damage` returns `EngineError::RedirectCycle`.

Differences from PLAN §4.2, §4.6, §10.3.3 and §10.3.4, settled here:

- The engine enums and constants are generated (`gen_engine_enums.py`) instead of written by
  hand, and `HistKind`, `EvKey`, `CtxKey` and `FrameKind` come from the call-site scan.
  `EvKey` holds only the 20 keys `emit` writes; an IR key outside them
  (`EventAmount.key`, `EventFlag.key`, `EventCard.key`) is decoded with
  `EvKey::from_value(reg.str(key))`, and `None` never matches, as Python's `ev` returns the
  default. No event key starts with `it`, so `push_frame`'s `it` vars (`interp.py:84-86`) can
  never be written.
- `DecisionHdr.prompt` is a `Prompt` (`Engine(EnginePrompt) | Ask(StrId) | Choose(VarId)`)
  rather than a `PromptId` (O4): the AskMay prompt is the node's `StrId` and the Choose prompt
  is its `VarId`, so no prompt table is needed. `canon::from_canonical` recovers `Ask` and
  `Choose` from the waiting frame's instruction.
- `IntKey::Loop` carries the `LoopInit.key` `VarId` (the generated node field type) instead of
  the parsed loop number; its rendered name is `Registry::var_name`.
- `view::derived`, `view::play_cost` and `observe::determinize` return `Result`: Python's
  `_compute` raises from `value`/`cond` (`MinOf([])` `ValueError`, `view.py:878`; checked
  arithmetic), and `_fit` raises `ValueError` (`observe.py:104`). `invariants::check` returns
  `CheckError`, which also carries an `EngineError` of its `derived` call.
- `new_game` returns `NewGameError` (construction, unimplemented card, or any error of
  `advance`, `game.py:157`) and takes `NewGameOptions { chooser, turn_limit, max_actions,
  validate }` with Python's `validate=True` keyword; `concede` cannot fail (`game.py:763-771`
  raises nothing).
- `view::matches` takes `&[FilterId]` (the typed handle); `view::rules_all`,
  `view::abilities_of`, `Derived::{empty, key, converged}`, `GameState::{seed, standby,
  setup_chooser, priority, passes, game_over, frame_ints, turn_limit, max_actions}`,
  `EventPayload::get`, `DecisionHdr::ctx`, `Prompt::text`, `FrameVars::{get, iter}`,
  `Action::to_json` and the L1 arena helpers are added, because Python reads those fields
  directly. `touch_for_ai` is dropped: the only AI write is `ai/actions.py:228-229`
  (`set_rested_for_trial`); `observe.py:82`, `:130` touch inside the engine.
- §10.3.4 corrections from the Python functions: `shields_receiving_damage(st, uids, amount,
  d)` (`core.py:681-697`, not `(p, n)`); `new_card` and `ability_snapshot` return `Result`
  (u16 uids; `derived`); `record` takes `player: u8`; `damage_only_battle` ends at
  `battle.py:282`; `place_ex_resource` and `queue_trigger` are not boundary names (each is
  used only inside its owner's layers). `add_lasting` and `deploy_cards` became boundary names
  in the review fixes, because C's `testkit::engine` wrappers call them.
- `HistKindRef` (PLAN §4.2) is a contract type with landed `decode` and `matches`; the
  generated `HappenedThisTurn.kind` stays a `StrId` (B1's node types cannot name engine
  enums), and B2 decodes it once per node.
- The review fixes add the public `new_game_by_number` and
  `deck_construction_problems_by_number` (Python's `DeckList` of strings: E's
  `replay::check_replay`, G's trace headers), the testkit hooks `install_view` and
  `set_force_link` (G's `state` and `force_link` records), the `testkit::engine` wrappers (G's
  `call` records) and the L1 zone-list helpers (B2's in-place DECK reorders).
- `canon::Snapshot` is `StateSnapshot` (Python's `core.Snapshot` is LKI, here `Lki`).
- The `Derived` starting layout uses `Vec<bool>` for `present` and `linked` and keeps traits as
  `TraitsRef { def, granted: Span }` into a `StrId` pool (a grant only appends,
  `view.py:383-384`); B2 may change any of it.
- `Lasting.player` is `i8`, `HistoryEvent.player` is `u8` and `HistoryEvent.by` is `i8`, all
  checked at their call sites; `TriggerInst.ability_key` is the named `AbilityKey`.
- Step 3 addition (B2): `view::link_satisfied_unpatched(reg, unit, pilot)`, the link check of
  `view.py:125-138` without `TestOverrides`. D2's `opening_score` needs it, because
  `ai/setup.py:18` binds the original function, so the `force_link` monkeypatch of
  `tests/cards/test_wp_gd05_a.py:101-113` never reaches the mulligan, while `view.py:269` and
  `ai/policy.py:200` see it. `link_satisfied(st, ..)` is the overridable one.

## gcg-ai (owners D1, D2) — PLAN §6.1-§6.3, §6.6, §10.3.5, Inv 07, Inv 08

Types (`crates/gcg-ai/src/contracts.rs`):

- the player interface: `Agent` (trait: `name`, `choose`, `choose_traced`, `decision_log`),
  `DecisionTrace` (what `choose_traced` reports), `AgentCx`
  (`scratch`, `par`, `cancel`), `Scratch` (128-byte aligned, `Default + Send + 'static` for
  the pool; D2's starting layout), `AgentError` (with `py_class`);
- bookkeeping and the log: `LOG_ALTERNATIVES`, `Alternative`, `AgentCore` (fields
  `pub(crate)`, D2's), `LogEntry`, `LogAlternative`;
- the D1 ↔ D2 boundary: `PolicyKnobs`, `BASELINE_KNOBS`, `FEATURES`, `N_FEATURES`,
  `ROLE_FEATURES`, `FeatureSet`, `Weights`, `WeightsError`, the tables `HAND_WEIGHTS`,
  `TUNED_WEIGHTS`, `ROLE_WEIGHTS`, `DEFAULT_WEIGHTS`, `Density`, `MoveKey`, `PruneFlags`,
  `NoopFlags`;
- shared by both searches and `gcg-diff`: `SearchResult`, `SearchStats`, `SearchEffort`,
  `IterTrace`,
  `SearchTracer` (trait), `NoTrace`;
- the baseline players: `GREEDY_MULLIGAN_SAMPLES`, `GREEDY_MAX_DECISIONS`, `RandomAgent`,
  `GreedyAgent` (fields `pub(crate)`, D1's).

Landed (S): `AgentError::py_class`, the default body of `Agent::choose_traced` (`choose` with
no trace, for agents without an `AgentCore`), `AgentCore::{new, name, seed, log_enabled,
decisions, entries}`, `PolicyKnobs::default` (= `BASELINE_KNOBS`), `FeatureSet::{ALL, as_str, from_value,
names}`, `Weights::{new, values, feature_set, as_pairs}`, `Display for WeightsError`, the
three weight tables (bit patterns tested against the live Python objects; `DEFAULT_WEIGHTS` is
the same object as `ROLE_WEIGHTS`), `NoTrace`, `RandomAgent::new`, `GreedyAgent::new`.

| Owner | Modules (files) | Python |
| --- | --- | --- |
| D1 | `policy`, `values`, `targeting`, `evaluation`, `playout`, `greedy`, `random` | `ai/policy.py`, `values.py`, `targeting.py`, `evaluation.py`, `playout.py`, `agents.py:18-66` |
| D2 | `agent`, `log`, `actions`, `fingerprint`, `noop`, `setup`, `scratch` | `ai/base.py`, `actions.py`, `noop.py`, `setup.py` |

D1's `weights.rs` of PLAN §10.1 is not needed: the tables are contract data. `lib.rs` is
lead-owned; each landing report names its `mod` lines.

```rust
// D1 — policy.rs, evaluation.rs, playout.rs (ai/policy.py, evaluation.py, playout.py)
pub fn option_scores(st: &GameState, dec: &DecisionView<'_>, options: &[Action],
                     knobs: &PolicyKnobs) -> Result<Vec<f64>, EngineError>;   // policy.py:104-108, scorers :388-400; options only from the argument (Inv 08 §1); no view argument: the scorers call view::derived where Python's do
pub fn choose(st: &GameState, rng: Option<&mut SplitMix64>, temperature: f64,
              knobs: &PolicyKnobs) -> Result<Action, EngineError>;              // policy.py:72-83
pub fn pick(options: &[Action], scores: &[f64], rng: Option<&mut SplitMix64>, temperature: f64) -> usize;   // policy.py:86-101 (the chosen index)
pub fn burst_density(st: &GameState, observer: u8) -> Density;                // evaluation.py:132-140
pub fn features(st: &GameState, player: u8, density: Density) -> Result<Vec<f64>, EngineError>;   // :231-241 (base)
pub fn role_features(st: &GameState, density: Density) -> Result<Vec<f64>, EngineError>;          // :280-300 (roles)
pub fn logistic(z: f64) -> f64;                                                // :303-307
pub fn score(st: &GameState, player: u8, w: &Weights, density: Density) -> Result<f64, EngineError>;      // :310-317
pub fn evaluate(st: &GameState, player: u8, w: &Weights, density: Density) -> Result<f64, EngineError>;   // :320-326
pub fn play_until_turn(st: &mut GameState, turn: i32, rng: Option<&mut SplitMix64>, temperature: f64,
                       max_decisions: u32, knobs: &PolicyKnobs) -> Result<u32, EngineError>;   // playout.py:12-26
pub fn play_to_end(st: &mut GameState, rng: Option<&mut SplitMix64>, temperature: f64,
                   max_decisions: u32, knobs: &PolicyKnobs) -> Result<u32, EngineError>;      // playout.py:29-41
// D1 — random.rs, greedy.rs (ai/agents.py:18-66, :104-127)
impl Agent for RandomAgent;                                                    // agents.py:28-32 (draws even for one option); overrides choose_traced
impl Agent for GreedyAgent;                                                    // agents.py:48-66; overrides choose_traced
pub fn make_random_agent(seed: u64, log: bool) -> Box<dyn Agent>;             // agents.py:120-121
pub fn make_greedy_agent(seed: u64, log: bool, weights: Weights) -> Box<dyn Agent>;   // agents.py:122-123

// D2 — agent.rs, log.rs (ai/base.py)
impl AgentCore {
    pub fn begin<'s>(&mut self, st: &'s GameState, player: u8)
        -> Result<(DecisionView<'s>, SplitMix64), AgentError>;                // base.py:60-65 (decisions += 1 before seeding; single options count)
    pub fn finish(&mut self, st: &GameState, player: u8, chosen: Action, alts: &[Alternative]) -> Action;   // base.py:67-76 (log_entry :79-100, before apply)
}
impl LogEntry { pub fn to_json(&self) -> Json; }                              // base.py:85-100 key order; a None label writes no key
pub fn card_label(st: &GameState, uid: i32, observer: u8) -> String;          // base.py:103-109
pub fn describe(st: &GameState, dec: &DecisionView<'_>, action: Action, observer: u8) -> String;   // base.py:112-122
pub fn uid_argument(dec: &DecisionView<'_>) -> bool;                          // base.py:125-131
// D2 — actions.rs, fingerprint.rs, noop.rs (ai/actions.py, noop.py)
pub fn ex_used(a: Action) -> i32;                                              // actions.py:41-45
pub fn action_key(st: &GameState, dec: &DecisionView<'_>, a: Action) -> MoveKey;   // actions.py:57-69
pub fn canonical(st: &GameState, dec: &DecisionView<'_>) -> Vec<(MoveKey, Action)>;   // actions.py:72-85
pub fn attack_can_matter(st: &GameState, player: u8, attacker: Uid) -> Result<bool, EngineError>;   // actions.py:106-120 (D1's policy.py:18 imports it)
pub fn prune_dominated(st: &GameState, dec: &DecisionView<'_>, moves: Vec<(MoveKey, Action)>,
                       f: PruneFlags) -> Result<Vec<(MoveKey, Action)>, EngineError>;   // actions.py:242-277 (`kept or moves`)
pub fn merge_equivalent(st: &GameState, dec: &DecisionView<'_>, moves: Vec<(MoveKey, Action)>)
    -> Result<Vec<(MoveKey, Action)>, EngineError>;                            // actions.py:379-393 (fingerprints :284-376, OnceKey rules PLAN §4.2)
pub fn without_effect(st: &GameState, player: u8, dec: &DecisionView<'_>, moves: &[(MoveKey, Action)],
                      seed: u64, knobs: &PolicyKnobs, f: NoopFlags, w: &Weights)
    -> Result<Vec<MoveKey>, EngineError>;                                      // noop.py:128-166 (a set: membership only)
// D2 — setup.rs (ai/setup.py)
pub fn opening_score(hand: &[&CardDef], first: bool, reg: &Registry) -> f64;  // setup.py:30-60 (view::link_satisfied_unpatched)
pub fn mulligan_choice(st: &GameState, player: u8, samples: u32, rng: &mut SplitMix64) -> (Action, Vec<Alternative>);   // setup.py:63-83
pub fn first_player_choice(st: &GameState, player: u8, games: u32, rng: &mut SplitMix64, knobs: &PolicyKnobs,
                           cx: &mut AgentCx<'_>) -> Result<(Action, Vec<Alternative>), AgentError>;   // setup.py:86-120
```

Behaviour fixed by the contract:

- Inside a pool task, the runner builds `AgentCx { scratch, par, cancel }` from the `&mut
  Scratch` and `&TaskCx` that `map_indexed` passes. Every other caller (D1/D2 scenario tests at
  M6, `gcg-search`'s null-config test, G's agent replay, tools) uses `Pool::run_inline`
  (landed, S), so no agent waits for E's pool:

  ```rust
  let cancel = Cancel::new();
  let action = Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
      let mut cx = AgentCx { scratch, par, cancel: &cancel };
      agent.choose(&st, player, &mut cx)
  })?;
  ```

  `cx.par.fork_join` needs E's body (M7). Until it lands, D2 may run `first_player_choice`'s
  simulations in list order on the calling thread, which gives the same sums (PLAN §6.2 item
  12, §7.3), and use `fork_join` once E has landed it. `crates/gcg-ai/src/contracts.rs` tests
  the pattern.
- Agents only read the true state; every simulation runs on a clone or a determinised world,
  and `view::derived(st)` calls on the true state fill its view cache as Python's do (PLAN
  D10, §4.6).
- `option_scores` computes the view itself, scorer by scorer, exactly where Python does:
  `_main` (`policy.py:159`), `_block` (`:260`), `_excess` (`:322`) and `_select` (`:352`) only
  when `ctx("mode") != 1`. The other scorers (`_action_step` `:288-298`, `_yes`, `_burst`,
  `_choose_first`, `_keep`, `_discard` `:326-331`, `_arrange` `:334-341`) and the kinds with
  no scorer (all zeros, `:105-107`; ORDER_TRIGGER and PAYMENT, `:388-400`) never call
  `view::derived`, so they neither fill the cache nor return its `EngineError`. Callers
  (`choose`, D2's MCTS priors at `mcts.py:305-317`) pass no view.
- `first_player_choice` draws its `games` seeds up front (`setup.py:98`); the `2 × games`
  simulations may run through `cx.par.fork_join`, each job owning its state clone and seed
  (O1), with the sums taken afterwards in list order (PLAN §6.2 item 12, §7.3).
- Searches and `first_player_choice` read `cx.cancel` between iterations and simulations and
  return `AgentError::Interrupted` when it is set; nothing else changes (PLAN §7.5).
- `opening_score` checks links with `view::link_satisfied_unpatched`: `setup.py:18` binds the
  original `link_satisfied`, so the `force_link` monkeypatch of
  `tests/cards/test_wp_gd05_a.py:101-113` reaches `view.py:269` and `policy.py:200` but never
  the mulligan.

Differences from PLAN §6.1 and §10.3.5, settled here:

- `burst_density` returns `Density = [f64; 2]`, Python's per-owner tuple
  (`evaluation.py:132-140`), and `score`/`evaluate` take it.
- `option_scores` has no `dv: &Derived` parameter (PLAN §10.3.5 has one). Python's
  `option_scores(st, dec, knobs)` (`policy.py:104`) takes no view, and only four scorers
  compute one. With a `dv` argument, every caller would compute the view first: D2's priors
  would fill a determinised world's cache, and could raise, for kinds that never read the view
  in Python. Otherwise D2 would need D1's private knowledge of which kinds read it. Either way
  breaks §4.6 and D10.
- `option_scores`, `choose`, `features`, `role_features`, `score`, `evaluate` and
  `merge_equivalent` return `Result<_, EngineError>`: they call `view::derived` or
  `play_cost` (O7). `first_player_choice` and both `search` functions return `AgentError`
  (interrupts, and the `apply(check=True)` of `setup.py:116`).
- Added because a consumer needs them: `features`, `role_features` (F's `tune`),
  `attack_can_matter` (D1's policy), `action_key`, `card_label`, `describe` (D2's log),
  `make_random_agent`, `make_greedy_agent` (F).
- `SearchResult` lives here as `{action, alternatives, stats: SearchStats}`. `stats` holds
  Python's third field, `iterations`, and adds `budget`, `cap`, `roots` and `effort`: the trace
  agent block (§8.2) and the effort counts (§6.6) need them, the reference, the `gcg-search`
  copy and `gcg-diff` share the type, and a `DecisionTrace` takes `stats` by move.
- `Agent::choose_traced` and `DecisionTrace` are added for `gcg-diff`'s agent replay (PLAN
  §8.7: every alternative unranked and unrounded, iterations, budget, cap, roots, `rng_after`;
  level 2 through the tracer). The decision log keeps only the top 5, rounded, and greedy's
  and random's alternatives exist only inside their `impl Agent`. D1 and D2 implement `choose`
  and `choose_traced` over one generic function per agent (`decide<T: SearchTracer + ?Sized>`),
  so both make the same choice, draws and log entry, and `choose` keeps `NoTrace`
  monomorphised. `DecisionTrace.k` and `seed` are `AgentCore`'s counter and the per-decision
  seed (`ai/base.py:64-65`), `rng_after` that generator's `state` when the decision returns.
- `LogEntry` labels are `Option`, because the test agents' entries have none
  (`tests/runner/random_agent.py:30-40`). `LogEntry::from_json` is E's
  (`gcg_runner::log_entry_from_json`), so `gcg-ai` needs no `serde_json`.
- The weight tables are contract data here, not D1's `weights.rs`.
- Searches take the tracer as a generic `&mut T` (`T: SearchTracer + ?Sized`), so `NoTrace`
  inlines away; `&mut dyn SearchTracer` still works.
- `AiTables` (W-33) is not a contract type (open item O10).

## gcg-ai-reference (owner D2) — PLAN §6.2, §6.4, §6.5, §10.3.5, Inv 07

Types and data (`crates/gcg-ai-reference/src/contracts.rs`): `WIN_DECAY`, `LEAF_FLOOR`,
`TIE_TOLERANCE`, `AGENT_PREFIX` (`mcts-`), `FACTORY_DESCRIPTION`
(`gcg-ai-reference::make_agent('mcts')`, A9), `ReferenceConfig` (the 24 `SearchConfig`
fields in Python's order), `ConfigError`, the presets `FAST`, `STANDARD`, `STRONG`,
`PRESETS`, `BASELINE_STANDARD`, `BASELINE_STRONG`, `BASELINE_PRESETS`, `BASELINE_WEIGHTS`
(the same object as `TUNED_WEIGHTS`), `TUNING_CONFIG`, and `ReferenceAgent` (fields
`pub(crate)`, D2's). Every preset field is spelled out; the tests compare each with the values
printed from the live Python objects (Inv 07 §2.1, floats as bits) and with Python's own
construction (`_adopted`, `config.py:60-77`), and a `const` assertion fails the build if a
shipped configuration breaks `__post_init__`. PLAN §6.5 item 5's `config.rs` is this file.

```rust
// Landed (S)
impl ReferenceConfig {
    pub const fn new(name: &'static str, max_iterations: u32, min_iterations: u32,
                     iterations_per_option: u32) -> ReferenceConfig;          // config.py:23-46 (Python's defaults)
    pub const fn check(&self) -> Result<(), ConfigError>;                     // config.py:48-53 (Python's message)
    pub fn budget(&self, n_options: usize) -> u32;                            // config.py:55-57 (exact; min wins over max)
    pub const fn knobs(&self) -> PolicyKnobs;                                 // policy.py:63-69 knobs_for
    pub fn agent_name(&self) -> String;                                       // agents.py:81 f"mcts-{config.name}"
}
impl fmt::Display for ConfigError; impl std::error::Error for ConfigError;
pub fn reference_preset(name: &str) -> Option<&'static ReferenceConfig>;    // PRESETS[name]: fast|standard|strong
pub fn baseline_preset(name: &str) -> Option<&'static ReferenceConfig>;     // BASELINE_PRESETS[name]: standard|strong
impl ReferenceAgent { pub fn new(config: ReferenceConfig, seed: u64, log: bool, weights: Weights) -> ReferenceAgent; }   // agents.py:73-83

// Frozen (D2) — mcts.rs, agent.rs (ai/mcts.py, ai/agents.py:69-127)
pub fn search<T: SearchTracer + ?Sized>(st: &GameState, player: u8, cfg: &ReferenceConfig, w: &Weights,
              rng: &mut SplitMix64, cx: &mut AgentCx<'_>, tracer: &mut T)
    -> Result<SearchResult, AgentError>;                                       // mcts.py:82-151 (+ :154-339; PLAN §6.2 items 1-11)
impl Agent for ReferenceAgent;                                                 // agents.py:85-98 (dispatch order of Inv 07 §1.3)
pub fn make_reference_agent(cfg: &ReferenceConfig, seed: u64, log: bool, w: &Weights) -> Box<dyn Agent>;   // agents.py:124-126 (clones cfg and w)
```

Differences from PLAN §6.4 and §10.3.5, settled here:

- `make_reference_agent` borrows its configuration and weights instead of taking `&'static`
  ones: `gcg-eval`'s `:field=value` overrides build configurations at run time
  (`ai/compare.py:98-112`), and agents are rebuilt every game (`runner/game.py:62-76`).
- `ReferenceConfig.name` is `Cow<'static, str>`, because `name=` is an allowed override
  (`ai/compare.py:76`); integer fields are `u32` (counts) and `i32` (`horizon_turns`), and
  F's parser rejects out-of-range values, which Python accepts and then fails or loops on.
  An override must call `check`, as `dataclasses.replace` re-runs `__post_init__`.
- `TUNING_CONFIG` is a `static` like the presets (`&'static` access).
- The `AgentFactory` of `gcg-eval reference-benchmark` (Python's `default_agent_factory`,
  `runner/agents.py:56-63`: `make_agent("mcts", preset=spec.preset, ...)` with
  `DEFAULT_WEIGHTS`) is F's: this crate cannot depend on `gcg-runner`.

## gcg-search (owner D2) — PLAN §6.4, §6.5, §10.3.5, §12.3

Types (`crates/gcg-search/src/contracts.rs`): `AGENT_PREFIX` (`search-`), `Preset`
(`Standard`, `Fast`; `ALL` in argparse order, `DEFAULT`, `as_str`, `from_value`, `FromStr`,
`agent_name`, `factory_description`, `mapping`, `help` with the brief's wording),
`UnknownPreset`, `StrengthenedConfig` (the 24 reference fields plus `weights`; no `Default`)
and `SearchAgent` (fields `pub(crate)`, D2's). All landed except the `Agent` impl. The W-136
`compile_fail` doctest (`StrengthenedConfig { ..Default::default() }`) is in `contracts.rs`;
stable rustdoc does not check error codes, so it was confirmed in a scratch crate to fail
with E0277 (`Default` not implemented) only.

```rust
// Frozen (D2) — presets.rs, mcts.rs, agent.rs
pub fn preset_config(p: Preset) -> &'static StrengthenedConfig;              // PLAN §6.4; v0: reference strong (standard) and reference standard (fast), renamed, ROLE_WEIGHTS, every field spelled out
pub fn search<T: SearchTracer + ?Sized>(st: &GameState, player: u8, cfg: &StrengthenedConfig,
              rng: &mut SplitMix64, cx: &mut AgentCx<'_>, tracer: &mut T)
    -> Result<SearchResult, AgentError>;                                       // v0: the reference's mcts.py:82-339 with cfg.weights
impl Agent for SearchAgent;                                                    // agent.rs only (lint §9.9 item 5); name search-{cfg.name}
pub fn make_agent(cfg: &StrengthenedConfig, seed: u64, log: bool) -> Box<dyn Agent>;   // clones cfg
// crates/gcg-search/tests/null_config.rs (D2; dev-dependency on gcg-ai-reference)
fn null_config(reference: &ReferenceConfig, weights: &Weights) -> StrengthenedConfig;   // field by field, every mechanism off
```

Differences from PLAN §10.3.5, settled here:

- `null_config` is a test helper, not a `gcg-search` function: taking a `ReferenceConfig`
  would make `gcg-search`, and with it `gcg-sim`, depend on `gcg-ai-reference`, which §2.2
  and §6.5 item 1 forbid. `gcg-search` has a dev-dependency on the reference instead;
  dev-dependencies never enter `gcg-cli`'s normal build, and the reachability test follows
  normal and build edges only. D2's null-config test reads the `search` traces with
  `gcg_diff::read_trace`; the lead adds the dev-dependency on `gcg-diff` when D2 lands it
  (cargo accepts a dev-dependency cycle for integration tests).
- The null-config test compares at the `search` level, with the reference's per-decision
  generator: the agent names are seed-path material and differ by design (`mcts-*`,
  `search-*`, A9).
- `StrengthenedConfig` carries `weights`, so a preset is self-contained (S10 refits weights
  per preset); `make_agent` therefore has no weights parameter, as in the plan.

## gcg-runner (owner E) — PLAN §7, §9.5, §10.3.6, §12.7, Inv 01, Inv 02

Types (`crates/gcg-runner/src/contracts.rs`):

- CLI constants shared by `gcg-sim` and `gcg-eval`: `EXIT_OK`, `EXIT_INVALID`, `EXIT_USAGE`,
  `EXIT_INTERRUPTED`, `INTERRUPTED_MESSAGE`, `DEFAULT_OUT_DIR`, `DEFAULT_REPLAYS`;
- configuration: `Role`, `Outcome`, `Format`, `Workers`, `SchemaVersion`, `BenchmarkConfig`,
  `AgentSpec`, `AgentFactory` (trait: `make`, `describe`);
- records: `PlayEvent`, `DutCardEvents`, `GameRecord`, `MatchRecord`, `RunTiming`,
  `BenchmarkRun`, `RunError`;
- reports and replays: `ReportPaths`, `SchemaError`, `ReportError`, `REPLAY_FORMAT`,
  `REPLAY_VERSION`, `ReplayCheck`, `ReplayError`;
- test agents: `TestAgentKind`, `TestRandomAgent`, `TestIllegalAgent`, `ILLEGAL_ACTION`.

Landed (S): `Role::as_str`, `Outcome::as_str`, `Format::{ALL, as_str, from_value,
games_per_match, wins_needed}`, `SchemaVersion::{as_str, id}`, `GameRecord::bench_seat`,
`BenchmarkRun::games`, `TestAgentKind::{ALL, as_str, factory_description}`,
`TestRandomAgent::new`.

Modules (E): `config`, `game`, `matches` (`match` is a keyword), `run`, `records`, `instrument`,
`reports` (`reports/{results,summary,aggregate,hypotheses,stats,schema,writer}.rs`), `replay`,
`test_agents`, `argparse` and `cli_support` (shared by `gcg-sim` and `gcg-eval`, open item
O11), and `clock` (the only wall-clock module, PLAN §9.9).

```rust
// config.rs (runner/config.py)
impl BenchmarkConfig {
    pub fn problems(&self) -> Vec<String>;                                    // config.py:47-61 (8 checks, Python's order and texts)
    pub const fn games_per_match(&self) -> u32;                               // :38-40
    pub const fn wins_needed(&self) -> u32;                                   // :42-44
}
pub fn dut_seat(m: u32) -> u8;                                                // config.py:64-66
pub fn seat_decks(cfg: &BenchmarkConfig, m: u32) -> Result<[DeckList; 2], DeckError>;   // :69-72 (to_decklist)
pub fn game_seed(master: u64, m: u32, g: u32) -> u64;                         // :75-76
pub fn agent_seed(master: u64, m: u32, g: u32, seat: u8) -> u64;              // :79-80
pub fn own_turn(turn: i32, seat: u8, first_player: u8) -> i32;                // :83-85
pub fn workers_used(cfg: &BenchmarkConfig, topology: &Topology) -> u32;       // run.py:83-85 processes_used; Auto via Topology::auto_workers (PLAN §7.4)
// game.rs, matches.rs, run.rs (runner/game.py, match.py, run.py)
pub fn play_game(cfg: &BenchmarkConfig, m: u32, g: u32, chooser: Option<u8>, f: &dyn AgentFactory,
                 cx: &mut AgentCx<'_>) -> Result<GameRecord, RunError>;    // game.py:106-166 (_make_agents :62-76, _drive :79-103)
pub fn play_match(cfg: &BenchmarkConfig, m: u32, f: &dyn AgentFactory,
                  cx: &mut AgentCx<'_>) -> Result<MatchRecord, RunError>;   // match.py:19-46
pub fn run_benchmark(cfg: &BenchmarkConfig, f: &dyn AgentFactory, progress: &mut dyn FnMut(u32, u32),
                     cancel: &Cancel) -> Result<BenchmarkRun, RunError>;     // run.py:109-140 (pool of workers_used threads, one Scratch each; W-149)
pub fn replay(record: &GameRecord, cfg: &BenchmarkConfig) -> Result<GameState, ReplayError>;   // game.py:185-194 (replay_game :169-182)
// records.rs (runner/records.py)
impl PlayEvent { pub fn to_json(&self) -> Json; }                            // records.py:26-27
impl DutCardEvents { pub fn to_json(&self) -> Json; }                        // :50-59
impl GameRecord {
    pub fn to_json(&self) -> Json;                                            // :107-132
    pub fn from_json(v: &serde_json::Value) -> Result<GameRecord, SchemaError>;   // :134-162 (export_records.py fixtures)
}
impl MatchRecord { pub fn to_json(&self) -> Json; }                          // :176-184
pub fn log_entry_from_json(v: &serde_json::Value) -> Result<LogEntry, SchemaError>;   // the dicts of ai/base.py:85-100 and tests/runner/random_agent.py:30-40
pub mod reports {                                                             // reports/*.py
    pub fn build_results(run: &BenchmarkRun) -> Json;                         // results.py:145-189 (1.2.0: PLAN §12.7)
    pub fn validate_results(doc: &Json, v: SchemaVersion) -> Result<(), SchemaError>;   // results.py:48-50; in-house subset validator (PLAN §9.5)
    pub fn render_summary(results: &Json, command: &str) -> String;           // summary.py:230-241
    pub fn reproduce_command(run: &BenchmarkRun) -> String;                   // writer.py:44-51; V1_2 runs write the corrected line (A23, W-100)
    pub fn games_ndjson(run: &BenchmarkRun) -> String;                        // writer.py:54-57
    pub fn select_replays(run: &BenchmarkRun) -> Vec<(&'static str, &GameRecord)>;   // results.py:86-91 ("win", "loss", "draw")
    pub fn replay_name(label: &str, g: &GameRecord) -> String;                // results.py:82-83
    pub fn to_json_text(doc: &Json) -> String;                                // writer.py:33-34
    pub fn write_reports(run: &BenchmarkRun, out: &Path, cancel: &Cancel)
        -> Result<ReportPaths, ReportError>;                                  // writer.py:94-110 (atomic :37-41, results.json last)
    pub mod stats {
        pub fn wilson(successes: u64, n: u64, z: f64) -> Option<(f64, f64)>; // stats.py:12-25 (None = Python's ValueError); F's statistics use it
        pub fn rnd(x: f64) -> f64;                                            // :28-29 round(x, 6)
    }
}
pub mod replay {                                                              // runner/replayfile.py
    pub fn describe_action(st: &GameState, dec: &DecisionView<'_>, a: Action) -> String;   // :45-65
    pub fn replay_document(record: &GameRecord, cfg: &BenchmarkConfig, label: &str) -> Result<Json, ReplayError>;   // :104-150
    pub fn check_replay(doc: &serde_json::Value) -> Result<ReplayCheck, ReplayError>;   // :164-205
}
impl fmt::Display for RunError; impl fmt::Display for ReplayError; impl fmt::Display for ReportError;
impl fmt::Display for SchemaError;                                            // (each with impl std::error::Error)
pub mod test_agents {                                                         // tests/runner/random_agent.py
    pub fn factory(kind: TestAgentKind) -> Box<dyn AgentFactory>;            // make_random_agent :47-48, make_illegal_agent :63-64
}
impl Agent for TestRandomAgent;                                               // random_agent.py:24-44
impl Agent for TestIllegalAgent;                                              // random_agent.py:56-60
```

Behaviour fixed by the contract:

- One match per pool task (PLAN §7.3); results by index, identical for any `--workers`; the
  reported failure is the lowest failing match index (W-149), and `Cancel` is SIGINT only.
- Agents are rebuilt for every game, seat 0 first (`runner/game.py:76`). The runner calls
  `choose` for every pending decision and slices each seat's new log entries after it
  (`:87-97`); Python's "mutated the game state" check (`:92-93`) cannot fail in Rust, because
  agents get `&GameState` (W-49).
- `timing.json` keeps Python's keys (A17); `RunTiming` is filled only by `clock.rs`.
- `replay::check_replay` builds each seat's deck as Python's `_decklist` does (the sorted
  expansion of the written numbers, `runner/replayfile.py:68-72`) and calls
  `gcg_engine::new_game_by_number`, so an unknown or wrong number fails as Python's
  `DeckConstructionError` and reports `malformed replay document: DeckConstructionError:
  player N: ...` (`replayfile.py:180-183`).

Differences from PLAN §10.3.6, settled here:

- `BenchmarkConfig` keeps Python's `ai_preset` (`config.py:26`), which `AgentSpec.preset`,
  `results.json` `ai.preset` and the reproduce line read; `AgentFactory::preset_label` is
  dropped.
- `mirrored_specs`, `comparison_specs` and their types are F's, in `gcg-eval`.
- `replay::verify_file` is Python's `check_replay` on a parsed document: reading and JSON
  errors are the CLI's messages (`cli.py:338-356`), and `ReplayReport` is Python's
  `ReplayCheck`.
- Added because a consumer needs them: `seat_decks`, `own_turn`, `workers_used`, `replay`,
  the record `to_json`/`from_json` functions, `log_entry_from_json`, the report helpers and
  `reports::stats`.
- The CLI constants live here, because `gcg-sim` and `gcg-eval` share them and neither binary
  crate can depend on the other.

## gcg-cli (owner E) — PLAN §2.3, §6.5, §7.5, §10.4 E

No contract types: the crate has no consumers. Its surface is Python's `gcg-sim`
(`cli.py:68-95`) with two differences only, `--ai-preset standard|fast` parsed by
`gcg_search::Preset` and the additive `--workers auto` (A5, A30, T-152). E lands, with real
bodies:

- the `[[bin]] gcg-sim` target, `src/main.rs` (`main` of `cli.py:386-399`: exit codes from
  `gcg_runner::EXIT_*`, `INTERRUPTED_MESSAGE` on SIGINT through
  `gcg_core::sys::sigint::install`) and `build.rs` with `build-support/code_digest.rs` (X19);
- the `gcg-sim` agent factory over `gcg_search::{preset_config, make_agent}`, whose
  `describe` is `Preset::factory_description`; `make` returns `Result<Box<dyn gcg_ai::Agent>,
  gcg_ai::AgentError>`, through the direct `gcg-ai` edge (Workspace, Edges);
- `src/lib.rs` keeps the module list; until E lands, the crate is a library with no items.

`crates/gcg-cli/tests/factory_types.rs` (S, landed) implements `gcg_runner::AgentFactory`
with `gcg-cli`'s dependencies only (a factory that parses the seat's preset and builds no
agent), so removing the `gcg-ai` edge fails the build.

`crates/gcg-cli/tests/no_reference.rs` (S, landed; T-022) enforces PLAN §2.2: it reads the
`Cargo.toml` of every workspace crate in `gcg-cli`'s closure (normal and build edges,
`[dependencies.<name>]` tables, `[target.<cfg>.*]` sections, optional entries,
`workspace = true` resolved through the root, renamed `package = ...` entries) and fails if
any lists `gcg-ai-reference` or `gcg-diff`. Its TOML-subset reader and walk have negative
controls on synthetic workspaces (direct, transitive, renamed, table form, target-specific
and build dependencies; dev-dependencies are not followed). It asserts that the walk reached
`gcg-runner`, `gcg-search`, `gcg-ai`, `gcg-engine`, `gcg-effects`, `gcg-data` and `gcg-core`,
and not `gcg-eval`.

## gcg-eval (owner F) — PLAN A14, A24, §10.4 F, §12.4-§12.6, Inv 08 §8-§10

Types (`crates/gcg-eval/src/contracts.rs`): `MIRROR_BLOCK`, `DECISION_LIMIT`, `SeedRole` (A14
ranges; `bounds`, `contains`, with the disjointness unit test that A14 asks for), `ChooseFirst`
(A24; default `seat0`), `DeckSet`, `PairSet`, `AgentDesc` (the parsed agent-spec grammar),
`MirroredSpec` (Python's `GameSpec`), `GameOutcome`, `SpecError`. Landed (S): the `as_str`,
`from_value`, `ALL` and `bounds`/`contains` accessors.

`gcg-eval` has no downstream consumers, so its functions are F's design; the list below is the
plan F starts from, and F reports any change in its landing diff:

```rust
pub fn parse_agent(text: &str) -> Result<AgentDesc, String>;                  // compare.py:79-113 + the ref:/search: grammar of PLAN §10.4 F
pub fn load_decks(set: DeckSet) -> Result<Vec<(String, DeckList)>, String>;   // compare.py:116-124 (named; examples, tests, tuning, eval, panel)
pub fn mirrored_specs(games: u32, master: u64, challenger: &AgentDesc, opponent: &AgentDesc,
                      decks: [&DeckList; 2]) -> Vec<MirroredSpec>;            // selfplay.py:88-110
pub fn comparison_specs(games: u32, seed: u64, challenger: &AgentDesc, opponent: &AgentDesc,
                        decks: &[DeckList]) -> Result<Vec<MirroredSpec>, SpecError>;   // compare.py:127-143
pub fn summarize(specs: &[MirroredSpec], outcomes: &[GameOutcome]) -> Json;   // compare.py:146-178
pub fn compare(challenger: &str, opponent: &str, games: u32, seed: u64, workers: Workers, decks: &str)
    -> Result<Json, String>;                                                  // compare.py:181-195 (stdout and --out bytes, PLAN §8.8 item 2)
impl fmt::Display for SpecError;                                              // "need at least two decks" / f"games must be a positive multiple of {n}"
```

Also F's: the `[[bin]] gcg-eval` target and `main.rs`, `build.rs` (X19), the
`reference-benchmark` factory (`make_reference_agent(reference_preset(spec.preset), seed, log,
DEFAULT_WEIGHTS)`, `describe` = `gcg_ai_reference::FACTORY_DESCRIPTION`), and the
subcommands of PLAN §2.1 and A30.

## gcg-diff (owner G) — PLAN §8.2, §8.7, §9.9-§9.10, §10.3.7

Types (`crates/gcg-diff/src/contracts.rs`), the trace format version 1 of PLAN §8.2:
`TRACE_FORMAT_VERSION`, `SNAPSHOT_EVERY`, `TraceKind`, `OptionsMode`, `Generator`,
`TraceDeck`, `Header`, `PendingRec`, `TraceOptions`, `Fingerprint`, `AltRec`, `AgentRec`,
`DecisionRec`, `SnapshotRec`, `IterRec`, `ApplyTraceRec`, `OutcomeRec`, `TraceArg`,
`EnumValue`, `CallSpec`, `NewGameSpec`, `ScenarioRec`, `ErrorRec`, `ApplyRec`, `CallRec`,
`DerivedMark`, `StateRec`, `ProbeRec`, `TestOutcome`, `Record` (15 variants, one per `"t"`
tag), `Trace`, `TraceError`; and the runner's `DiffOptions`, `ReplayStats`, `Divergence` and
`EngineUnderTest` (trait). Each type documents its JSON keys. The format is the contract
between G's Python exporters and every Rust reader: a change bumps `"v"` and goes through
the change protocol.

```rust
pub fn parse_record(line: &str) -> Result<Record, String>;                    // one NDJSON line (§8.2 encoding: float.hex, u64 ≥ 2^53 as strings)
pub fn read_trace(path: &Path) -> Result<Vec<Trace>, TraceError>;             // §10.3.7; a file holds one or more traces
pub fn options_sha16(options: &[Action]) -> String;                          // §8.2 (pinned by crates/gcg-diff/tests/vectors/)
pub fn replay_trace(t: &Trace, opts: &DiffOptions, engine: &mut dyn EngineUnderTest)
    -> Result<ReplayStats, Divergence>;                                       // §8.7 (stops at the first divergence)
impl fmt::Display for Divergence;                                             // the §8.7 report
impl fmt::Display for TraceError;
pub struct MockEngine;  impl EngineUnderTest for MockEngine;                  // M3 gate: replays the recorded observations (G's layout)
```

`EngineUnderTest` has `new_game`, `load`, `load_state`, `set_registry`, `force_link`,
`pending`, `observe`, `apply`, `canonical`, `probe` and `call`, one per record kind that
changes or reads the engine, and `state`, the current `&GameState` for the agent replay
(`None` from `MockEngine`). The real engine implements them with:

- `new_game` and a `scenario` record's `new_game`: `gcg_engine::new_game_by_number` on the
  `TraceDeck` strings;
- `load_state`: `canon::from_canonical`, then `testkit::install_view` for a stale view, then
  `testkit::set_force_link` with the current list;
- `set_registry`: `Registry::with_fragment` on the embedded registry; `force_link`:
  `testkit::set_force_link`;
- `call`: the name dispatcher maps the Python functions `interp.{push_frame, add_lasting,
  draw, deploy_cards}`, `game.advance` and `core.card_type` (whatever alias or import the test
  used) to `testkit::engine::*`, `core.clear_listen_cache` to a no-op (gcg-engine testkit
  section), and the public names to the public API. `{"$": NodeId}` arguments become handles
  through `Registry::{filter_id, cond_id, continuous_id}`.

Agent replay (PLAN §8.7 "Search and baselines traces", drill-down levels 1-2):

- `replay_trace` builds one agent per seat at the header, with `log = true`, the seat's
  `agent_seeds` entry and the constructor the generator used: `gcg_ai::make_random_agent`,
  `gcg_ai::make_greedy_agent` (`DEFAULT_WEIGHTS`), or `gcg_ai_reference::make_reference_agent`
  with `reference_preset(preset)` and `DEFAULT_WEIGHTS`, or for `baseline-*` with
  `baseline_preset(..)` and `BASELINE_WEIGHTS` (`tests/ai/test_baseline.py:27-33`). The mapping
  from `Header.agents` and `preset` is G's.
- At a decision record with an agent block, after the pending, option and fingerprint checks
  and before `apply`, it calls `agent.choose_traced(engine.state(), player, cx, tracer)` with
  a context from `Pool::run_inline` (gcg-ai section) and compares, in order: the action with
  `chosen`; `DecisionTrace.{k, seed, rng_after}`; every alternative (action, value bits,
  visits) with `alts`; `search` with `iters`/`budget`/`cap`/`roots` (both present or both
  absent); then the bytes of the seat's new `decision_log()` entries. A `None` trace is a
  divergence. The tracer is `NoTrace`, except for `trace-search` at the requested decision,
  where G's recorder produces the `iter` and `apply_trace` records to compare.
- With `engine.state() == None` (`MockEngine`) the agent blocks are decoded and not replayed;
  `ReplayStats.agent_decisions` counts the replayed ones.

Differences from PLAN §10.3.7, settled here: `replay_trace` takes the engine under test
explicitly (the real engine or `MockEngine`); the `state` record has its own `StateRec`
(it carries `derived`, `view_from` and `view_key`, §8.2), `view` keeps the signature JSON;
`call` and `probe` records share `CallSpec` with optional `kwargs`; `roots` in the agent
block are move keys and `alts` are all alternatives in agent order, unranked and unrounded;
an `iter` record's `decision` is the decision record's `i`.

## Open items (for the lead)

| Id | Item | Proposal |
| --- | --- | --- |
| O1 | `TaskCx::fork_join` as written in PLAN §7.1 (borrowed `f`, non-`'static` inputs, run by already-running pool workers) cannot be implemented in safe Rust | Frozen above with owned (`'static`) inputs and an `Arc` closure; E confirms on landing. The alternative, a fresh `std::thread::scope` per fork, keeps the borrowed signature but does not let idle workers help |
| O2 | `gcg_core::pyrepr::repr_str` rejects non-ASCII text; the effect `repr` needs `Ⅱ` and `①` (PLAN §5.7) and the deck messages need CPython's `isprintable` rule (Inv 03 §10 H1) | B1 adds an IR-table-driven variant in `gcg-effects` (PLAN §5.7) and A one over its generated Unicode tables; `gcg-core` stays unchanged |
| O3 | `py_max2`/`py_min2` (PLAN §2.1) do not exist in `gcg-core` yet | Frozen above, owner S, lands before D1 needs them |
| O4 | The engine-literal half of the prompt table (`PromptId`) is engine text, but `Registry::prompt` owns the table | **Settled in step 2:** the engine does not use `PromptId`. `DecisionHdr.prompt` is `Prompt::{Engine(EnginePrompt), Ask(StrId), Choose(VarId)}`: the 14 literal prompts are the generated `EnginePrompt`, the AskMay prompt is the node's `StrId` (`AskMay.prompt`) and the Choose prompt is `"choose " + var_name(Choose.var)`. **Closed in the review fixes:** `PromptId`, `Registry.prompts` and `Registry::prompt` are dropped (the IR has no prompt table) |
| O5 | `getrandom` is in `Cargo.lock` as a build-time dependency of `cc` (via `jobserver`), used only by the `mimalloc`/`zstd` build scripts; the lint of PLAN §9.9 item 3 scans sources, not the lock file | Keep; no crate source names it, and it is never linked into a binary |
| O6 | The engine needs reverse lookups the step-1 `Registry` API lacks: the `VarId` of its literal frame-variable names (PLAN §3.3 already requires them in `var_names`), the `StrId` of a card number for `ContRef::AbilityGrant` (`interp.py:1549`) and snapshot loading, and the `FilterSetId` of an `ApplyPlayer` filter tuple (`interp.py:1314`) | **Settled in step 2:** `Registry::{var_id, str_id, filters_key}` added to the gcg-effects signatures (additions only; nothing B1 has landed changes). The exporter must put every `ApplyPlayer.filters` tuple into `filter_sets` |
| O7 | `view::derived` returns `Result` (Python's `_compute` can raise), so every caller of the view propagates `EngineError`, including the AI's calls on the true state | **Settled in step 3:** `AgentError::Engine(EngineError)`; the D1/D2 functions that reach the view return `Result<_, EngineError>`, the agent-level ones `AgentError` |
| O8 | The `Derived` and `GameConst` starting layouts are untested against the census (PLAN §8.6): capacities (`EVENT_CAPACITY` 10, `CONTEXT_CAPACITY` 4, `ONCE_KEY_CAPACITY` 6) are the plan's estimates | The Phase 2 census confirms or raises them; a raise is a contract change (lead) |
| O9 | Results schema 1.2.0 (PLAN §12.7) needs per-seat effort totals (`ai.effort`) and the search block (`ai.search`), but neither `Agent` nor `AgentFactory` exposes them, and the 1.2.0 schema file is a Phase 6 deliverable | Until Phase 6 lands schema 1.2.0, `gcg-sim` writes 1.1.0 (its `ai.preset` values `standard`/`fast` are in the 1.1.0 enum). The P6 owner then adds `Agent::effort(&self) -> SearchEffort` and `AgentFactory::search_block(&self) -> Option<Json>` through the change protocol; `SearchEffort` already exists |
| O10 | W-33 `AiTables`: Python memoises polarity per `(program, pc)` and harm per `def_id` in process globals (`ai/targeting.py:54-112`, `ai/actions.py:167-199`); PLAN §6.2 and A16 place the tables per registry, but no frozen D1/D2 signature carries them and `Registry` cannot hold AI types | Phase 3 recomputes them (pure functions of the registry, so exact by construction). A table type is introduced through the change protocol only if the A16 plugin records a test that reads an entry computed before an override, or Phase 5 measures a win |
| O11 | The argparse-compatible parser and the CLI pieces both binaries share (progress printer `cli.py:143-159`, deck loading and its messages `:102-126`, `data status` `:236-331`, the report-directory check `:176-185`) live in `gcg-runner` (`argparse`, `cli_support`), because `gcg-eval` cannot depend on the `gcg-sim` crate; their API is not frozen | E publishes the API in its M7 landing report and the lead freezes it here before F's CLI work (M8) |
| O12 | The W-135 `compile_fail` doctest for `Agent` (an agent cannot `apply` on the true `&GameState`) needs `gcg_engine::apply`, which lands with C's L5; a doctest written now would fail for an unresolved name instead | Lands with T-023 at M7 in `crates/gcg-ai/src/contracts.rs` (lead); the W-136 doctest is in place now, the W-49 one since step 2 |
| O13 | PLAN §13 M2 runs `uv run mypy --strict port/oracle`, which fails with `import-untyped` on every `gcg_sim` import: the package has no `py.typed`, and mypy finds it only as an installed package. With `MYPYPATH=src` the three oracle scripts pass `--strict`. Fixing it needs a file outside this milestone's scope: `PLAN.md` or `pyproject.toml` | The lead adds `mypy_path = "src"` to `[tool.mypy]` in `pyproject.toml` (checked: `MYPYPATH=src uv run mypy` still reports no issues in 101 source files), or writes `MYPYPATH=src` into the PLAN §13 M2 line. The gate above uses `MYPYPATH=src` until then |

## Change log

| Date | Change | By |
| --- | --- | --- |
| 2026-11-01 | Step 1: workspace, `gcg-core`, `gcg-data`, `gcg-effects` | lead (S) |
| 2026-11-01 | Step 2: `gcg-engine` contracts (`contracts.rs`, generated `enums.rs`, `gen_engine_enums.py`, the self dev-dependency for `testkit`); O4 settled (`Prompt`); O6 settled (`Registry::{var_id, str_id, filters_key}` added to gcg-effects) | lead (S) |
| 2026-11-01 | Step 3: `contracts.rs` of `gcg-ai`, `gcg-ai-reference` (presets as data), `gcg-search`, `gcg-runner`, `gcg-eval`, `gcg-diff`; `crates/gcg-cli/tests/no_reference.rs`; `gcg-search` dev-dependency on `gcg-ai-reference`; `gcg-eval` direct edges to `gcg-effects`, `gcg-engine`; `view::link_satisfied_unpatched` added to gcg-engine (B2); `null_config` moved to `gcg-search`'s tests; `mirrored_specs`/`comparison_specs` moved to `gcg-eval`; O7 settled; O9-O12 opened | lead (S) |
| 2026-11-01 | Review fixes (see the review log): gate commands `--check`/`--locked --offline` and the mypy line (O13); `Pool::run_inline` landed in `gcg-core` and the `AgentCx` pattern documented; gcg-engine `HistKindRef` (landed), `new_game_by_number`, `deck_construction_problems_by_number`, `testkit::{install_view, set_force_link}`, `testkit::engine::*` with `FrameArgs`/`LastingArgs`, the L1 `zone_remove`/`zone_extend`/`zone_insert_front`, `add_lasting`/`deploy_cards` as boundary names; gcg-effects `Registry::{filter_id, cond_id, continuous_id}`, `PromptId`/`Registry.prompts`/`Registry::prompt` dropped (O4 closed), `ContentId` field `pub(crate)`; `EngineUnderTest::{load_state, set_registry, force_link}`; mypy fixes in `port/oracle` | lead (S) |
| 2026-11-01 | Second review fixes (see the review log): `gcg-cli` → `gcg-ai` edge with `crates/gcg-cli/tests/factory_types.rs`; gcg-ai `Agent::choose_traced` (default body landed), `DecisionTrace`, `SearchStats` (`SearchResult.{iterations, budget, cap, roots, effort}` move to `SearchResult.stats`); gcg-diff `EngineUnderTest::state` and the agent-replay procedure | lead (S) |
| 2026-11-01 | Third review fix (see the review log): gcg-ai `option_scores` drops its `dv: &Derived` parameter (Python `policy.py:104`); the scorers compute the view where Python's do; recorded in the differences from PLAN §10.3.5 | lead (S) |

## Review log

Review of the contracts milestone, 2026-11-01 (12 findings). Each was checked against the code
before it was fixed; none was rejected outright. Parts not done as the reviewer proposed:

| Finding | Decision | Reason |
| --- | --- | --- |
| mypy `--strict port/oracle` (minor) | Fixed the five new errors and the older `export_vectors.py:32` one; the gate's invocation is O13 | The command as PLAN §13 writes it can only pass with `mypy_path` in `pyproject.toml` or `MYPYPATH=src` in PLAN; both files are the lead's, outside this milestone's files |
| `testkit::engine` wrappers (major) | Added the six names the census finds; no `emit` wrapper | An AST scan of `tests/` for calls through engine module aliases and imported names finds no direct `core.emit` call, so a wrapper would have no caller |
| `ContentId::as_str` (minor) | Field made `pub(crate)`; `as_str` stays `&str` | With only `from_hex` and the loader constructing one, the bytes are ASCII by construction, so B1's body needs no fallible path a caller can reach |
| AgentCx construction (major) | `Pool::run_inline` landed (S); E's schedule unchanged | Scheduling E before M6 would edit PLAN §10.5 (the lead's) and still leave D waiting; `run_inline` unblocks every agent test now, and only `fork_join` waits for E |

Second review, 2026-11-01 (2 findings, both major). Both were reproduced before the fix: a
test with `gcg-cli`'s dependency set failed with E0432 (unresolved import `gcg_ai`), and no
frozen item gave `gcg-diff` the unranked, unrounded alternatives, `rng_after` or a state to
run an agent on. Neither was rejected. Parts not done as the reviewer proposed:

| Finding | Decision | Reason |
| --- | --- | --- |
| `gcg-cli` cannot name `gcg_ai::{Agent, AgentError}` (major) | Direct `gcg-ai` edge, recorded in the Edges row; PLAN §2.2's `gcg-cli = ...` line is not edited | PLAN.md is the lead's file, outside this milestone's files; the Edges row already lists the other additions to §2.2 (`gcg-eval`, `gcg-search`). The edge adds no crate to `gcg-cli`'s closure, so `no_reference.rs` and the `cargo tree` check are unchanged. The edge was chosen over a re-export from `gcg-runner`, which would make one crate's API carry another's types |
| No frozen agent introspection for `gcg-diff` (major) | `choose_traced(.., tracer: &mut dyn SearchTracer) -> Result<(Action, Option<DecisionTrace>), AgentError>` with a default body, plus `EngineUnderTest::state`; no `last_decision` | One method serves level 1 and level 2 (the tracer reaches the search through D2's dispatch). The result is `Option` with a default of `choose` and `None`, so the runner's test agents, which have no decision counter or per-decision seed, need no invented values; a `None` on an agent-block decision is a divergence, so a missing override cannot pass unnoticed. `SearchResult`'s counts moved into `SearchStats` so the trace takes them without a clone |

Third review, 2026-11-01 (1 finding, major). It was checked against the code before the fix:
`option_scores(st, dec, knobs)` at `ai/policy.py:104` takes no view; `grep -n 'V.derived'
ai/policy.py` finds only `:159` (`_main`), `:260` (`_block`), `:322` (`_excess`) and `:352`
(`_select`, after its `mode == 1` return). `V.cdef` (`engine/view.py:23-24`) and `card_worth`
(`ai/values.py:36`) do not reach the view, and ORDER_TRIGGER and PAYMENT have no scorer
(`policy.py:388-400`, `engine/types.py:101-114`). No Rust code used the `dv` parameter. The
finding was not rejected, and the fix is as the reviewer proposed, with one exception:

| Finding | Decision | Reason |
| --- | --- | --- |
| `option_scores` takes `dv: &Derived`, which forces a view compute that Python does not do (major) | `dv` dropped from the frozen signature; the per-scorer view calls are a contract bullet; the change is in the change log and in the list of differences from PLAN §10.3.5 in this file. PLAN §10.3.5's line is not edited | PLAN.md is the lead's file, outside this milestone's files; this file's difference list is where §10.3.5 deviations are recorded |
