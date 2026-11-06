//! Contract types of `gcg-diff` (docs/dev/PLAN.md §8.2, §8.7, §10.3.7), owned by the lead
//! (§10.1 S): the trace records of format version 1 and the differential runner's interface.
//!
//! A trace file is NDJSON, one compact object per line with sorted keys and `ensure_ascii`,
//! written by `oracle/traces.py` and `oracle/pytest_trace.py` (G). A file holds one
//! or more traces; each starts with a `header` record. Encoding rules (§8.2): floats are
//! `float.hex()` strings, ints are JSON numbers except u64 values of 2^53 and above, which are
//! decimal strings; actions are `[kind value, a, b, c]` (`engine/state.py:21-24`). Every record
//! carries its tag in `"t"`; the variant comments below give the tag and the JSON keys.
//!
//! Workstream G implements the reader, the replay and the readers' error texts with the
//! signatures frozen in docs/dev/contracts.md (gcg-diff section); only the lead edits this
//! file (§10.6 change protocol). Python sources are cited relative to `src/gcg_sim/`.

use std::path::PathBuf;

use gcg_ai::{IterTrace, MoveKey};
use gcg_data::DefId;
use gcg_effects::{AbilityId, ProgramId};
use gcg_engine::{Action, DecisionKind, EndReason, GameState, StateSnapshot, Step};

/// `"v"` of every header this reader accepts.
pub const TRACE_FORMAT_VERSION: u32 = 1;
/// Every 32nd decision stores a full snapshot (PLAN A21).
pub const SNAPSHOT_EVERY: u32 = 32;

// ---------------------------------------------------------------------------------------------
// Header

/// `"kind"` of a header (§8.2).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TraceKind {
    /// A game from `new_game` with recorded choices (sets `full`, `vanilla`, `fuzz`,
    /// `coverage`, `baselines`, `worlds`).
    Game,
    /// One pytest test (set `tests`, PLAN §8.2 per-test records).
    Test,
    /// A game whose agent decisions the Rust agents must reproduce (set `search`).
    Search,
}

/// `"options_mode"` (§8.2, A21): tier 1 is always `full`; `hash` only in tier 2.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum OptionsMode {
    Full,
    Hash,
}

/// `"generator"`: what produced the trace. A header whose `ir_sha256` differs from the
/// embedded IR is refused (§8.4).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Generator {
    /// The repository commit of the generating Python tree.
    pub git: String,
    /// `platform.python_version()`, `3.15.1` on the golden platform (A1).
    pub python: String,
    /// Python's `code_digest()` (`reports/versions.py:44-53`).
    pub code_digest: String,
    pub ir_sha256: String,
    /// SHA-256 of `data/SOURCES.lock.json`.
    pub data_lock: String,
    /// `PYTHONHASHSEED` of the run (1 or 2, W-150).
    pub hash_seed: u32,
    /// `GCG_ORACLE_EFFECT_OFFSET` (0 unless the numbering-shift probe ran, §8.9).
    pub effect_offset: u32,
    /// The largest uid of the game, which the probe compares with the offset.
    pub max_uid: u32,
}

/// One seat's deck in a header: card numbers in the caller's order (`new_game` shuffles them
/// in that order, so the order is RNG-relevant; `gcg_data::DeckList`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct TraceDeck {
    /// JSON `"main"`.
    pub main: Vec<String>,
    /// JSON `"res"`.
    pub resources: Vec<String>,
}

/// `"t": "header"`. Game and search traces carry `decks`, `seed`, `chooser`, the limits and
/// the agents; a test trace may lack `decks` and `seed` (its state comes from the `scenario`
/// record).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Header {
    /// `"v"`, [`TRACE_FORMAT_VERSION`].
    pub version: u32,
    /// `"id"`, e.g. `fuzz/s1/g0017`; unique in the corpus.
    pub id: String,
    pub kind: TraceKind,
    pub generator: Generator,
    pub decks: Option<[TraceDeck; 2]>,
    pub seed: Option<u64>,
    /// `new_game(chooser=...)`: `null`, 0 or 1.
    pub chooser: Option<u8>,
    pub turn_limit: i32,
    pub max_actions: i64,
    /// Agent names per seat (`random`, `policy`, `greedy`, `mcts-<preset>`, the fuzz agent).
    pub agents: Vec<String>,
    pub agent_seeds: Vec<u64>,
    /// `null`, `fast`, `standard`, `strong`, `baseline-standard` or `baseline-strong`.
    pub preset: Option<String>,
    pub options_mode: OptionsMode,
}

// ---------------------------------------------------------------------------------------------
// Game records

/// A pending decision as `Decision.to_json()` writes it (`engine/state.py:51-58`), used by
/// `apply` and `call` records and by [`EngineUnderTest::pending`]. Context keys stay strings,
/// so a new Python key is a reported difference, not a decode failure.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PendingRec {
    pub player: u8,
    pub kind: DecisionKind,
    pub options: Vec<Action>,
    pub prompt: String,
    pub context: Vec<(String, i32)>,
}

/// The option list of a decision record: in full, or as `"options_sha16"`, the first 16 hex
/// digits of `sha256(json.dumps([o.to_json() for o in options], separators=(",", ":"),
/// ensure_ascii=True))` (§8.2).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TraceOptions {
    Full(Vec<Action>),
    Sha16(String),
}

/// The state fingerprint compared at every decision (PLAN A21, §8.7 step 2).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Fingerprint {
    pub version: u64,
    /// `"rc"`: Python's `_rules_checked`.
    pub rules_checked: i64,
    /// `"rng"`: the game RNG state.
    pub rng: u64,
    pub action_count: i64,
    pub event_group: i64,
    pub turn: i32,
    pub step: Step,
    /// `"digest16"`: the first 16 hex digits of `state_digest` before the action is applied.
    pub digest16: String,
}

/// One alternative of an agent block: `[action, mean as float.hex(), visits]`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AltRec {
    pub action: Action,
    pub value: f64,
    pub visits: u32,
}

/// `"agent"` of a decision record (drill-down level 1, §8.7): present for agent decisions of
/// `search` and `baselines` traces. The search fields are absent for decisions the agent took
/// without a search (one option, mulligan, play/draw, greedy, random). The replay compares it
/// with the [`gcg_ai::DecisionTrace`] that `Agent::choose_traced` returns for the same
/// decision (docs/dev/contracts.md, gcg-diff section).
#[derive(Clone, PartialEq, Debug)]
pub struct AgentRec {
    /// `"k"`: the agent's decision counter (`ai/base.py:64`).
    pub k: u64,
    /// `"seed"`: `derive_seed(agent_seed, name, "decision", k)` (`ai/base.py:65`).
    pub seed: u64,
    /// `"rng_after"`: the per-decision generator's state after `choose`.
    pub rng_after: u64,
    /// `"iters"`, `"budget"`, `"cap"`: [`gcg_ai::SearchStats`].
    pub iterations: Option<u32>,
    pub budget: Option<u32>,
    pub cap: Option<u32>,
    /// `"roots"`: the root move keys `[kind, a, b, variant]` in order.
    pub roots: Option<Vec<MoveKey>>,
    /// `"alts"`: every alternative, unranked and unrounded, in the agent's order.
    pub alternatives: Vec<AltRec>,
    /// `"log"`: the entries the seat's `decision_log()` gained with this decision, as Python
    /// wrote them (`oracle/traces.py` `agent_block`, trace lines have sorted keys); the
    /// replay compares them value by value with the Rust agent's new entries.
    pub log: Vec<serde_json::Value>,
}

/// `"t": "decision"`: the state before the `i`-th decision and the recorded choice.
#[derive(Clone, PartialEq, Debug)]
pub struct DecisionRec {
    /// `"i"`: decision index in the trace, from 0.
    pub index: u32,
    pub player: u8,
    pub kind: DecisionKind,
    pub prompt: String,
    pub context: Vec<(String, i32)>,
    pub options: TraceOptions,
    /// `"n"`: the number of options.
    pub n: u32,
    pub chosen: Action,
    /// `"rng"`, `"version"`, `"rc"`, `"action_count"`, `"event_group"`, `"turn"`, `"step"`,
    /// `"digest16"`, flattened into the record.
    pub fingerprint: Fingerprint,
    pub agent: Option<AgentRec>,
}

/// `"t": "snapshot"` (A21): the full canonical state at decision `i` (game start and end,
/// every [`SNAPSHOT_EVERY`]th decision, after any change outside `apply`, every decision of
/// the `full` set), with the side tables that `canon::from_canonical` needs.
#[derive(Clone, PartialEq, Debug)]
pub struct SnapshotRec {
    /// `"i"`.
    pub index: u32,
    /// `"state"`, `"continuous"`, `"filter_sets"`, `"rules_checked"`.
    pub snapshot: StateSnapshot,
}

/// `"t": "iter"` (drill-down level 2): one iteration of the search at decision `decision`.
#[derive(Clone, PartialEq, Debug)]
pub struct IterRec {
    /// `"decision"`: the `i` of the decision record the search belongs to.
    pub decision: u32,
    /// `"n"`, `"child"`, `"k"`, `"fresh"`, `"path_len"`, `"applies_walk"`,
    /// `"applies_rollout"`, `"leaf"`, `"terminal"`, `"total_before"`, `"total_after"`.
    pub iteration: IterTrace,
    /// `"world16"`: the digest prefix of the world the iteration played in.
    pub world16: String,
}

/// `"t": "apply_trace"` (drill-down level 3): the `j`-th engine apply inside iteration `n`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ApplyTraceRec {
    pub decision: u32,
    pub n: u32,
    pub j: u32,
    pub action: Action,
    pub after16: String,
}

/// `"t": "outcome"`: the end of a game trace.
#[derive(Clone, PartialEq, Debug)]
pub struct OutcomeRec {
    /// `null` (unfinished), -1 (draw), 0 or 1.
    pub winner: Option<i8>,
    pub end_reason: Option<EndReason>,
    pub turn: i32,
    /// `"actions"`: actions applied.
    pub actions: u64,
    /// `"final_digest"`: the full `state_digest`.
    pub final_digest: String,
    /// `"state"`: the final canonical state.
    pub state: serde_json::Value,
}

// ---------------------------------------------------------------------------------------------
// Per-test records (§8.2)

/// An argument of a `scenario`, `call` or `probe` record (§8.2 "Argument encoding",
/// `oracle/pytest_trace.py` `TestTrace.enc`). Tuples, lists, `Action`s (`to_json()`),
/// dicts (`[key, value]` pairs in insertion order), sets (sorted), a `Ctx`
/// (`[controller, host, card_uid, vars, event]`) and a `RuleInst` (`[{"$": rule}, controller,
/// source, key, aux]`) are JSON lists; a float is its `float.hex()` string and an int of
/// 2^53 and above a decimal string.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TraceArg {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<TraceArg>),
    /// `{"enum": "<Name>", "value": <value>}`: an engine or DSL enum member, the value as
    /// Python's (a number for `IntEnum`s, a string for `StrEnum`s).
    Enum {
        name: String,
        value: EnumValue,
    },
    /// `{"$": <NodeId>}`: a DSL object the test passed, as a node of the embedded registry
    /// with the trace's current `override` fragment applied (the base's own id for a node the
    /// base IR holds).
    Node(u32),
    /// `{"enum": "<Type>", "value": ...}` with one of the [`EngineRef`] type names: an engine
    /// object passed by reference.
    Ref(EngineRef),
    /// A `Decision` (`Decision.to_json()`, `engine/state.py:51-58`).
    Decision(PendingRec),
}

/// An engine object passed by reference (`TestTrace.enc`); no engine or DSL enum has one of
/// these type names (the exporter checks).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum EngineRef {
    /// `{"enum": "GameState", "value": "<digest16>"}`: the replay's current state, whose
    /// `state_digest` prefix this is.
    State(String),
    /// `{"enum": "GameState", "value": "scenario"}`: the state of the `Scenario` being built
    /// (only inside a `scenario` record's `calls`).
    ScenarioState,
    /// `{"enum": "Derived", "value": "current"}`: the current state's view (`view.derived`).
    View,
    /// `{"enum": "CardDef", "value": <def_id>}`.
    CardDef(DefId),
    /// `{"enum": "AbilityEntry", "value": <aid>}`.
    Ability(AbilityId),
    /// `{"enum": "Frame", "value": <index>}`: the frame at this index of the current state's
    /// frames; -1 when the frame was none of them.
    Frame(i32),
    /// `{"enum": "Program", "value": <ProgramId>}`: a `Program`, or a Python program id passed
    /// as a parameter named `program_id`, as a program of the embedded registry with the
    /// current fragment applied.
    Program(ProgramId),
    /// `{"enum": "Registry", "value": "current"}`.
    Registry,
}

/// The `value` of a [`TraceArg::Enum`].
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum EnumValue {
    Int(i64),
    Str(String),
}

/// One captured call: a `testkit.Scenario` method (`testkit.py:38-182`) or an engine function.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CallSpec {
    /// `"fn"`: `<module>.<function>` after the engine module the test reached the function
    /// through (`core`, `interp`, `battle`, `observe`, `game`, `invariants`, `view`), e.g.
    /// `interp.push_frame`, `view.ap_of`; `testkit.<function>` for a testkit query;
    /// `Scenario.<method>` for a Scenario method, `Scenario.__init__` included.
    pub func: String,
    pub args: Vec<TraceArg>,
    /// `"kwargs"`, in call order; absent means none.
    pub kwargs: Vec<(String, TraceArg)>,
}

/// One call of a `scenario` record's build, with what it raised: a `Scenario` method, or an
/// engine call on the Scenario's state before `start` ([`EngineRef::ScenarioState`]).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ScenarioCall {
    /// `"fn"`, `"args"`, `"kwargs"`.
    pub call: CallSpec,
    /// `"error"`: present when the call raised (the test caught it).
    pub error: Option<ErrorRec>,
}

/// `new_game(decks, seed, *, chooser, turn_limit, max_actions, validate)`
/// (`engine/game.py:107-116`): a game trace's header, or how a test built its game when it did
/// not use a `Scenario`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct NewGameSpec {
    pub decks: [TraceDeck; 2],
    pub seed: u64,
    pub chooser: Option<u8>,
    pub turn_limit: i32,
    pub max_actions: i64,
    /// `validate` (Python's default `True`; a game trace's header always validates).
    pub validate: bool,
}

/// `"t": "scenario"`: the test id, its traceability tags, and how the test built its state:
/// the Scenario calls up to `Scenario.start` (`testkit.py:38-182`), or the `new_game` spec.
#[derive(Clone, PartialEq, Debug)]
pub struct ScenarioRec {
    /// `"test"`, the pytest node id.
    pub test: String,
    /// `"tags"`: the `rule`/`card`/`faq`/`ruling` marks, as `kind:value`.
    pub tags: Vec<String>,
    /// `"calls"`: from `Scenario.__init__` to `Scenario.start`; empty with `new_game`.
    pub calls: Vec<ScenarioCall>,
    pub new_game: Option<NewGameSpec>,
    /// `"before16"`: the digest prefix of the state before `advance` in `Scenario.start`
    /// (`testkit.py:181`); absent with `new_game`.
    pub before16: Option<String>,
    /// `"after16"`: the digest prefix after `advance`, or after `new_game`.
    pub after16: String,
    /// `"calls_reproduce"`: false when the test wrote the Scenario's state between recorded
    /// calls, so that only `snapshot` rebuilds the state before `advance`.
    pub calls_reproduce: bool,
    /// `"rules_checked"`: `_rules_checked` of the state before `advance` (of the new game with
    /// `new_game`); always present.
    pub rules_checked: i64,
    /// `"state"`, `"continuous"`, `"filter_sets"` (with `rules_checked`): the state before
    /// `advance` (the new game with `new_game`). Present when `calls_reproduce` is false, and
    /// in every record of a trace written with `--trace-snapshots all`.
    pub snapshot: Option<StateSnapshot>,
}

/// `{"type": "<Python class>", "message": "..."}` of an `apply` or `call` that raised.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ErrorRec {
    /// JSON `"type"`, e.g. `IllegalActionError`.
    pub class: String,
    pub message: String,
}

/// `"t": "apply"`: an `apply` outside any agent. Like a decision record, it describes the
/// state the action is applied to (`before16`, `pending`); `after16` covers the state after.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ApplyRec {
    pub action: Action,
    pub check: bool,
    /// The digest prefix before the apply.
    pub before16: String,
    /// The digest prefix after the apply (whether or not it raised).
    pub after16: String,
    /// The pending decision before the apply, which the action answers (`null`: none).
    pub pending: Option<PendingRec>,
    pub error: Option<ErrorRec>,
}

/// `"t": "call"`: a direct call from a test module into an engine function of `core`,
/// `interp`, `battle`, `observe`, `game` or `invariants`, replayed through `gcg-diff`'s name
/// dispatcher; a name without a dispatcher entry is a counted failure. A `new_game` that raised
/// is a `call` named `game.new_game` with no arguments and its spec in `new_game`, replayed
/// with [`EngineUnderTest::new_game`].
#[derive(Clone, PartialEq, Debug)]
pub struct CallRec {
    pub call: CallSpec,
    /// `null` when the call raised.
    pub result: serde_json::Value,
    /// The current state's digest prefix after the call; `""` while no state is current.
    pub after16: String,
    pub error: Option<ErrorRec>,
    /// `"new_game"`: the spec of a `game.new_game` that raised.
    pub new_game: Option<NewGameSpec>,
}

/// `"derived"` of a `state` record (`engine/view.py:232-239`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum DerivedMark {
    /// No view is cached.
    None,
    /// The cached view matches the state.
    Fresh,
    /// Python still holds a view computed before the test's direct write.
    Stale,
}

/// `"t": "state"`: the test mutated the state directly between engine calls. With
/// [`DerivedMark::Stale`], Rust computes the view from `view_from` and installs it under
/// `view_key` before the next call (§8.2).
#[derive(Clone, PartialEq, Debug)]
pub struct StateRec {
    pub snapshot: StateSnapshot,
    pub derived: DerivedMark,
    pub view_from: Option<serde_json::Value>,
    pub view_key: Option<serde_json::Value>,
}

/// `"t": "probe"`: a `testkit.*` or `view.*` query of a test, in call order (probes call
/// `derived`, so Rust evaluates them in the same order), or one of the two probes of a `views`
/// trace, `views.cards` and `V.trigger_index` (`oracle/traces.py` `generate_view`).
#[derive(Clone, PartialEq, Debug)]
pub struct ProbeRec {
    pub call: CallSpec,
    /// `null` when the query raised.
    pub result: serde_json::Value,
    /// `"error"`: present when the query raised.
    pub error: Option<ErrorRec>,
}

/// `"t": "view"`: `tests/paranoid.py:24-39`'s view signature after an apply, or what computing
/// it raised (`{"sig": null, "error": ...}`).
#[derive(Clone, PartialEq, Debug)]
pub struct ViewRec {
    /// `"sig"`: `null` when computing it raised.
    pub sig: serde_json::Value,
    pub error: Option<ErrorRec>,
}

/// `"kind"` of an `override` record.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum OverrideKind {
    /// Right after the header: the nodes, programs and filter sets of the test's own objects
    /// (the arena of every `{"$": NodeId}` and [`EngineRef::Program`] argument).
    Additions,
    /// After `registry.override_script` (`registry.py:209-215`).
    Override,
    /// After `registry.restore_entry` (`registry.py:218-220`).
    Restore,
}

/// One read of an AI memo-table entry that existed when an override started (PLAN A16).
#[derive(Clone, PartialEq, Debug)]
pub struct MemoRead {
    /// `"table"`, e.g. `gcg_sim.ai.targeting._cache`.
    pub table: String,
    /// `"key"`: the entry's key, a process-local program id written as its content id.
    pub key: Vec<serde_json::Value>,
    pub value: serde_json::Value,
}

/// `"memo"` of an override: whether every AI memo table was empty when the override started
/// (each override test runs in a fresh process, A16), the table sizes, and the reads of the
/// entries that existed then.
#[derive(Clone, PartialEq, Debug)]
pub struct MemoCheck {
    pub empty: bool,
    /// `"sizes"`: entries per table, by table name in name order.
    pub sizes: Vec<(String, u64)>,
    pub reads: Vec<MemoRead>,
}

/// `"t": "override"`: from here on, states are built on the embedded registry with
/// `fragment` applied (`Registry::with_fragment`; the format of `oracle/export_ir.py`
/// `build_fragment`, `fragment_version` 1). Every fragment of a test holds the test's own
/// objects with the same ids.
#[derive(Clone, PartialEq, Debug)]
pub struct OverrideRec {
    pub kind: OverrideKind,
    /// The IR fragment, applied to the base registry (not to the previous fragment).
    pub fragment: serde_json::Value,
    /// Present exactly on [`OverrideKind::Override`].
    pub memo: Option<MemoCheck>,
}

/// `"outcome"` of an `end` record.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TestOutcome {
    Passed,
    Failed,
}

/// One trace record (PLAN §8.2, §10.3.7).
#[derive(Clone, PartialEq, Debug)]
pub enum Record {
    /// `header`.
    Header(Header),
    /// `snapshot`.
    Snapshot(SnapshotRec),
    /// `decision`.
    Decision(DecisionRec),
    /// `iter`.
    Iter(IterRec),
    /// `apply_trace`.
    ApplyTrace(ApplyTraceRec),
    /// `outcome`.
    Outcome(OutcomeRec),
    /// `scenario`.
    Scenario(ScenarioRec),
    /// `apply`.
    Apply(ApplyRec),
    /// `call`.
    Call(CallRec),
    /// `state`.
    State(StateRec),
    /// `view`.
    View(ViewRec),
    /// `probe`.
    Probe(ProbeRec),
    /// `override`.
    Override(OverrideRec),
    /// `force_link`: `{"units": [card numbers]}` (`tests/cards/test_wp_gd05_a.py:101-113`).
    ForceLink(Vec<String>),
    /// `end`: `{"outcome": "passed"|"failed"}`.
    End(TestOutcome),
}

/// One trace: its header and the records after it, in file order.
#[derive(Clone, PartialEq, Debug)]
pub struct Trace {
    pub header: Header,
    pub records: Vec<Record>,
}

/// A trace file that cannot be decoded. `line` counts from 1; 0 means the file itself.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct TraceError {
    pub path: PathBuf,
    pub line: usize,
    pub detail: String,
}

// ---------------------------------------------------------------------------------------------
// The differential runner (§8.7)

/// Options of one trace replay (`gcg-diff corpus` flags, §8.7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct DiffOptions {
    /// `--parse-only`: decode records and check hashes only (the M3 gate).
    pub parse_only: bool,
    /// `--roundtrip`: load every snapshot with `from_canonical`, re-serialise it and compare
    /// the canonical bytes and digest (the M5.1 gate).
    pub roundtrip: bool,
    /// `--paranoid`: replay on a `paranoid` build (W-49, W-124).
    pub paranoid: bool,
}

/// What a passing replay covered.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ReplayStats {
    pub records: u64,
    pub decisions: u64,
    pub snapshots: u64,
    pub applies: u64,
    pub calls: u64,
    pub probes: u64,
    /// Agent blocks the Rust agent reproduced (`Agent::choose_traced` against a real
    /// engine; always 0 with `MockEngine`).
    pub agent_decisions: u64,
}

/// The first divergence of a trace (§8.7 "Divergence report"); `Display` (G) is the report
/// of §8.7.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Divergence {
    pub trace_id: String,
    /// The 1-based line of the record where Python and Rust first differ.
    pub line: usize,
    /// The decision index, turn, step, player and kind, when the record is a decision.
    pub decision: Option<u32>,
    pub turn: Option<i32>,
    pub step: Option<Step>,
    pub player: Option<u8>,
    pub kind: Option<DecisionKind>,
    /// The first differing field, e.g. `version` or `cards[66].zone`.
    pub field: String,
    pub python: String,
    pub rust: String,
    /// The last action applied before the divergence.
    pub last_action: Option<Action>,
    /// A hint such as `an extra or missing touch() since decision 42`.
    pub note: Option<String>,
}

/// The engine side of a replay (§10.3.7): the real engine, or a `MockEngine` that replays the
/// recorded observations (the M3 gate, before the engine exists).
pub trait EngineUnderTest {
    /// `new_game` with a spec: a game trace's header (decks, seed, chooser, limits,
    /// `validate = true`), a `scenario` record's `new_game`, or a `game.new_game` call record.
    /// On an error the current state stays.
    ///
    /// # Errors
    /// What `new_game` raised, as Python's class and message.
    fn new_game(&mut self, spec: &NewGameSpec) -> Result<(), ErrorRec>;
    /// Replace the state by a snapshot (`canon::from_canonical`).
    ///
    /// # Errors
    /// A snapshot that does not load.
    fn load(&mut self, snapshot: &StateSnapshot) -> Result<(), ErrorRec>;
    /// A `state` record (§8.2): replace the state by `rec.snapshot`. With
    /// [`DerivedMark::Stale`], also load `rec.view_from` (with the snapshot's side tables) and
    /// install its view as the state's cached view under `rec.view_key`
    /// (`gcg_engine::testkit::install_view`) before the next call; with `Fresh` or `None` the
    /// cache starts empty. The current force-link list stays applied.
    ///
    /// # Errors
    /// A snapshot that does not load, or a view whose key is not `rec.view_key`.
    fn load_state(&mut self, rec: &StateRec) -> Result<(), ErrorRec>;
    /// An `override` record's fragment (PLAN §3.4): the following `new_game`, `load`,
    /// `load_state` and Scenario calls use `base.with_fragment(fragment)`, where `base` is the
    /// embedded registry. A state built before the record keeps its registry; every override
    /// test builds its states inside its override (`tests/ai/test_root_moves.py:188-192`,
    /// `:312-318`, `tests/rules/test_turn.py:848-849`).
    ///
    /// # Errors
    /// A fragment that `Registry::with_fragment` rejects.
    fn set_registry(&mut self, fragment: &serde_json::Value) -> Result<(), ErrorRec>;
    /// A `force_link` record: these Unit card numbers count as linked with any paired card in
    /// the current state and every later one (`gcg_engine::testkit::set_force_link`), as the
    /// monkeypatch of `tests/cards/test_wp_gd05_a.py:101-113` does, whose set the test may
    /// fill after the state exists. With no state loaded the list is kept for the next state
    /// (and the `Scenario` being built): four per-test traces send `force_link` first.
    ///
    /// # Errors
    /// An engine that cannot apply the list.
    fn force_link(&mut self, units: &[String]) -> Result<(), ErrorRec>;
    /// The pending decision, or `None`.
    fn pending(&self) -> Option<PendingRec>;
    /// The current state, which the replay hands to the seat's agent at a decision with an
    /// agent block (`Agent::choose_traced`, before `apply`). The real engine returns `Some`
    /// once a game or snapshot is loaded; `MockEngine` has no state and returns `None`, so
    /// agent blocks are decoded but not replayed against it.
    fn state(&self) -> Option<&GameState>;
    /// The fingerprint of the current state.
    fn observe(&self) -> Fingerprint;
    /// `apply(st, action, check=check)`.
    ///
    /// # Errors
    /// What `apply` raised.
    fn apply(&mut self, action: Action, check: bool) -> Result<(), ErrorRec>;
    /// `canonical_state(st)` (`runner/digest.py:36-49`).
    fn canonical(&self) -> serde_json::Value;
    /// A testkit or view query (`probe` records).
    ///
    /// # Errors
    /// What the query raised.
    fn probe(&mut self, call: &CallSpec) -> Result<serde_json::Value, ErrorRec>;
    /// A direct engine call (`call` records), or a call of a `scenario` record's build (a
    /// `Scenario` method, the replay's `game.advance` of a `calls_reproduce: false` snapshot).
    /// `Scenario.start` answers `{"digest16": <digest prefix>, "rules_checked": <n>}` of the
    /// state before its `advance` (`testkit.py:181`), which the replay compares with the
    /// record's `before16` and `rules_checked`; the other Scenario results are not compared.
    ///
    /// # Errors
    /// What the call raised, or a dispatcher miss.
    fn call(&mut self, call: &CallSpec) -> Result<serde_json::Value, ErrorRec>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_shareable_between_corpus_jobs() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Trace>();
        send_sync::<TraceError>();
        send_sync::<Divergence>();
        fn object_safe(_: &mut dyn EngineUnderTest) {}
        let _ = object_safe;
        assert_eq!(
            DiffOptions::default(),
            DiffOptions {
                parse_only: false,
                roundtrip: false,
                paranoid: false
            }
        );
    }
}
