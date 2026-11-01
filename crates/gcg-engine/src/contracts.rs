//! Contract types of `gcg-engine` (docs/port/PLAN.md §4.2-§4.6, §10.3.3, §10.3.4), owned by the
//! lead (§10.1 S). The enums and the module constants of the Python engine are generated into
//! `enums.rs` by `port/oracle/gen_engine_enums.py`.
//!
//! Workstreams C (layers L1, L3, L5) and B2 (L2, L4) implement the functions frozen in
//! docs/port/contracts.md (gcg-engine section) with real bodies; only the lead edits this file
//! (§10.6 change protocol). Three kinds of type live here:
//!
//! - **Records** with `pub` fields (`Action`, `CardInstance`, `Battle`, `Lasting`, `TriggerInst`,
//!   `Frame`, `HistoryEvent`, `DecisionHdr`, ...): their layout is frozen. Other crates read them
//!   through `&` borrows of the state and can never write into a `GameState` (W-49).
//! - **The shared state** `GameState`: its `pub(crate)` fields are frozen like a record,
//!   because both C and B2 read and write them inside the crate. The arenas
//!   (zones, frame `vars`/`ints`/`buf`, lasting targets, triggers, `once_used`, `pending`) are
//!   changed only through C's L1 helpers frozen in docs/port/contracts.md; plain scalar fields
//!   and the `cards`/`battles` records (and `rng`, `interp.py:426`, `:534`) are written
//!   directly, as Python does. The types of the internal layer boundary (`MoveOpts`,
//!   `DamageOpts`, `PushFrame`, `LastingOpts`, `EngineVars`, `UidBuf`) are frozen the same way.
//! - **Opaque types** with `pub(crate)` fields (`Derived`, `TriggerIndex` for B2; `ZoneLists`,
//!   `GameConst`, `FrameVars`, `Lki`, `InfoSetKey`, `CardMultiset`, `Scenario` for C): the
//!   fields are the lead's starting layout. The named owner may change them in its landing
//!   diff, because no other crate and no other owner reads them; their methods are frozen in
//!   docs/port/contracts.md.
//!
//! Python sources are cited relative to `src/gcg_sim/`; Inv 05 is
//! docs/port/inventory/05-engine-state-view.md and Inv 06a/06b the engine inventories.
//!
//! JSON forms. Every record documents its `to_json` list or key order; `canon::canonical_state`
//! (C, L1) writes exactly those, with the content-id substitutions of `runner/digest.py:36-49`
//! (PLAN §4.7). Bools are 0/1 in `CardInstance`, `Battle` and `TriggerInst`, and `true`/`false`
//! in `Frame` and `GameState.redraws` (Inv 05 §7 hazard 24).
//!
//! A field of the state cannot be written outside the crate (W-49):
//!
//! ```compile_fail
//! fn cheat(st: &mut gcg_engine::GameState) {
//!     st.turn = 7;
//! }
//! ```

use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

use gcg_core::rng::SplitMix64;
use gcg_core::small::InlineVec;
use gcg_data::{CardDb, DefId};
use gcg_effects::{
    AbilityId, Ev, EvSet, FilterSetId, KwAmounts, ProgramId, Registry, RuleModId, StrId, VarId,
};

pub use gcg_effects::{ContRef, Duration};

use crate::enums::{
    ActionKind, CtxKey, DecisionKind, EndReason, EnginePrompt, EvKey, FrameKind, HistKind, Phase,
    Step, Zone,
};

/// A card's index in `GameState.cards` (Python `uid`, `core.py:119-131`). Cards are never
/// removed: a token that leaves play moves to `Zone::Outside` and keeps its slot (PLAN §4.1).
/// Python writes uids into `i32` slots (`pair`, `host`, `Battle.target`) where `-1`
/// ([`crate::NO_ARG`]) or `-2` ([`crate::PLAYER_TARGET`]) mean "none" or "the player".
pub type Uid = u16;

// ---------------------------------------------------------------------------------------------
// Constants that Python writes as literals (the named ones are generated into enums.rs)

/// `GameState.turn_limit` default (`engine/state.py:507`) and `new_game(turn_limit=200)`
/// (`engine/game.py:112`).
pub const DEFAULT_TURN_LIMIT: i32 = 200;
/// The derived-view fixpoint runs at most this many passes: `for _ in range(6)`
/// (`engine/view.py:421`; Inv 05 §5.6).
pub const FIXPOINT_PASSES: usize = 6;
/// W-47: `advance` fails with [`EngineError::NoProgress`] after this many loop iterations
/// without a decision (Python would loop forever, `engine/game.py:734-760`).
pub const NO_PROGRESS_STEPS: u64 = 1_000_000;
/// Number of [`Ev`] members (`effects/dsl.py:692-727`).
pub const N_EVENTS: usize = Ev::ALL.len();
/// Inline pairs of an [`EventPayload`]. At most 8 occur at any emit site (Inv 06a §5.4); the
/// Phase 2 census (PLAN §8.6) confirms the capacity. A push beyond it is
/// [`EngineError::Capacity`], never a silent drop.
pub const EVENT_CAPACITY: usize = 10;
/// Inline pairs of [`DecisionHdr::context`]; Python's longest context has 2 pairs
/// (`(("min", mn), ("max", n))`, `engine/interp.py:433`).
pub const CONTEXT_CAPACITY: usize = 4;
/// Inline elements of a [`OnceKey`]; Python's longest key has 6
/// (`(tag, -1, effect_key, serial, uid, zone_seq)`, `engine/core.py:524`, `:561`).
pub const ONCE_KEY_CAPACITY: usize = 6;

// ---------------------------------------------------------------------------------------------
// Plain records (engine/state.py)

/// `Action(kind, a=-1, b=-1, c=-1)` (`engine/state.py:14-32`), a frozen dataclass: equality and
/// hashing compare all four fields. JSON `[kind.value, a, b, c]` (`:23-24`). Parameter meanings
/// per kind: `engine/types.py:117-135`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Action {
    pub kind: ActionKind,
    pub a: i32,
    pub b: i32,
    pub c: i32,
}

impl Action {
    /// `Action(kind, a, b, c)`.
    #[must_use]
    pub const fn new(kind: ActionKind, a: i32, b: i32, c: i32) -> Self {
        Self { kind, a, b, c }
    }

    /// `Action(kind)`: every parameter is [`crate::NO_ARG`].
    #[must_use]
    pub const fn of(kind: ActionKind) -> Self {
        Self::new(kind, crate::NO_ARG, crate::NO_ARG, crate::NO_ARG)
    }
}

/// A run of an arena (`items[start..start + len]`), PLAN §4.1 item 4.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    pub const EMPTY: Self = Self { start: 0, len: 0 };
}

/// `CardInstance` (`engine/state.py:71-141`), one physical card or token; `uid` is its index.
/// Fields in Python's order without `uid`; `__init__` defaults (`:92-102`): not rested, damage
/// 0, pair -1, zone_seq 0, entered_turn 0, known 0.
///
/// JSON (`:118-130`): `[uid, def_id, owner, int(zone), int(rested), damage, pair, zone_seq,
/// entered_turn, known]`, with `rested` as 0/1.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CardInstance {
    /// Mutable: determinization (`observe.py:76-80`, `:126-127`) and Unit variants
    /// (`interp.py:930`, `core.py:89-90`) rewrite it.
    pub def_id: DefId,
    pub owner: u8,
    pub zone: Zone,
    pub rested: bool,
    pub damage: i32,
    /// For a Unit in BATTLE the pilot uid, for a card in PAIRED the unit uid, else -1.
    pub pair: i32,
    /// Incremented on every zone change (rule 4-1-5).
    pub zone_seq: i32,
    pub entered_turn: i32,
    /// Bitmask `1 << player` of the players who know the identity ([`crate::BOTH_KNOW`] = 3).
    pub known: u8,
}

/// `Battle` (`engine/state.py:144-191`), in Python's field order; defaults `blocked=False`,
/// `damage_only=False`, `attack_triggers=True`, `ended=False`.
///
/// JSON (`:173-185`): `[attacker, attacker_seq, target, target_seq, defender, battle_id,
/// int(blocked), int(damage_only), int(attack_triggers), int(ended)]`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Battle {
    pub attacker: i32,
    pub attacker_seq: i32,
    /// A uid, or [`crate::PLAYER_TARGET`].
    pub target: i32,
    /// 0 when the target is the player (`battle.py:22`).
    pub target_seq: i32,
    pub defender: u8,
    pub battle_id: i32,
    pub blocked: bool,
    pub damage_only: bool,
    pub attack_triggers: bool,
    pub ended: bool,
}

/// `TriggerInst.ability_key` (`engine/state.py:274`): `(a.def_id, a.aid, int(a.unit_text))`
/// (`core.py:280`; the second element is the global aid, Inv 05 §7 hazard 23), `(-1, -1, 0)`
/// for delayed triggers (`core.py:400`), `(-2, 0, 0)` Repair and `(-3, 0, 0)` Breach
/// (`core.py:456`, `:479`), `(c.def_id, entry.burst_aid, 0)` for a Burst (`core.py:721`).
/// JSON `[def_id, aid, unit_text]` with `unit_text` as 0/1.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AbilityKey {
    pub def_id: i32,
    pub aid: i32,
    pub unit_text: bool,
}

/// An event tuple (`core.py:328-333`): `("subject", s), ("player", p), ("group", g)`, then the
/// remaining keyword arguments sorted by key ([`EvKey`]'s derived order is string order).
/// `ctx.ev(key)` / `_payload` return the first match (`view.py:46-50`, `core.py:187-191`).
/// Stored verbatim in `TriggerInst.event` and `Frame.event`; JSON `[[key, value], ...]`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct EventPayload(pub InlineVec<(EvKey, i32), EVENT_CAPACITY>);

/// One element of a once-per-turn key. Python keys are int tuples; the only element Rust does
/// not hold as an int is the process-local `effect_key` of a lasting rule key (PLAN §4.2, X5).
/// The derived `Ord` is arbitrary but total; Python's int order is never reproduced (§4.2).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum OnceElem {
    Int(i32),
    /// Only at position 2 of `(-8 | -7, -1, effect, serial, uid, zone_seq)`; written as the
    /// effect's content id (`runner/digest.py:30-33`).
    Cont(ContRef),
}

impl Default for OnceElem {
    /// The value of unused inline slots; never observable.
    fn default() -> Self {
        Self::Int(0)
    }
}

/// An entry of `st.once_used` (`engine/state.py:497`), with Python's element positions:
///
/// - `(aid, host, zone_seq)`: `core.py:236-237`, `game.py:439`, `:892`;
/// - `(tag, host, zone_seq)`: `wp_gd05_a.py:106-107`;
/// - `(-8 | -7, aid, host, uid, zone_seq)`: `core.py:524`, `:561` with a constant's rule key
///   `(aid, host)` (`view.py:436`);
/// - `(-8 | -7, -1, Cont(effect), serial, uid, zone_seq)`: the same with a lasting rule key
///   (`view.py:451`);
/// - `(-9, uid, turn)`: `interp.py:648`, `:674`.
///
/// JSON: a list; the canonical form sorts the keys by their `json.dumps` text
/// (`runner/digest.py:48`, PLAN §4.7). The derived `Ord` (element-wise, shorter prefix first)
/// is arbitrary but total.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct OnceKey(pub InlineVec<OnceElem, ONCE_KEY_CAPACITY>);

impl PartialOrd for OnceKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OnceKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_slice().cmp(other.0.as_slice())
    }
}

/// An element of the fingerprint form of a once key, `tuple(_SELF if x == uid else x for x in
/// key)` (`ai/actions.py:309`); `_SELF = -(1 << 40)` (`:281`) does not fit an `i32`, so it is
/// its own variant. Equality and a total order only; never serialised.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum FpOnceElem {
    Int(i32),
    Cont(ContRef),
    SelfRef,
}

impl Default for FpOnceElem {
    /// The value of unused inline slots; never observable.
    fn default() -> Self {
        Self::Int(0)
    }
}

/// The fingerprint form of a [`OnceKey`] (`ai/actions.py:308-310`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct FpOnceKey(pub InlineVec<FpOnceElem, ONCE_KEY_CAPACITY>);

impl PartialOrd for FpOnceKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FpOnceKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_slice().cmp(other.0.as_slice())
    }
}

/// `Lasting` (`engine/state.py:194-262`): a lasting effect in `st.lasting` (`Apply`,
/// `ApplyPlayer`) or a delayed trigger in `st.delayed` (`interp.py:1548-1557`). Python's field
/// order; defaults `player=-1`, `uses=0`, `filters_key=-1`, `aux=-1`, `serial=-1`.
///
/// JSON (`:231-245`): `[effect, controller, source_uid, [[uid, seq], ...], duration.value,
/// created_turn, battle_id, player, uses, filters, aux, serial]`; the canonical form writes the
/// effect's content id at position 0 and the filter set's content id, or -1, at position 9
/// (`runner/digest.py:39-43`).
#[derive(Clone, Copy, Debug)]
pub struct Lasting {
    /// Replaces the process-local `effect_key` (`effects/registry.py:124-130`, PLAN §5.6).
    pub effect: ContRef,
    pub controller: u8,
    pub source_uid: i32,
    /// `(uid, zone_seq)` pairs in `GameState::lasting_targets`.
    pub targets: Span,
    /// Python stores `duration.value` (`interp.py:1309`).
    pub duration: Duration,
    pub created_turn: i32,
    pub battle_id: i32,
    /// `>= 0` marks a player-level effect.
    pub player: i8,
    pub uses: i32,
    /// Replaces the process-local `filters_key` (`registry.py:132-138`); `None` for -1.
    pub filters: Option<FilterSetId>,
    pub aux: i32,
    pub serial: i32,
}

/// `TriggerInst` (`engine/state.py:265-322`), Python's field order; defaults `burst=False`,
/// `once_key=()`, `optional=False` (`optional` is never set, Inv 06a §2).
///
/// JSON (`:294-306`): `[program, controller, host, host_seq, card_uid, [def_id, aid,
/// unit_text], [[key, value], ...], int(burst), [once_key...], int(optional)]`; the canonical
/// form writes the program's content id at position 0 (`runner/digest.py:46-47`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TriggerInst {
    pub program: ProgramId,
    pub controller: u8,
    pub host: i32,
    pub host_seq: i32,
    pub card_uid: i32,
    pub ability_key: AbilityKey,
    pub event: EventPayload,
    pub burst: bool,
    /// `None` for Python's `()`.
    pub once_key: Option<OnceKey>,
    pub optional: bool,
}

/// `Frame` (`engine/state.py:325-411`), a resolving program, in Python's field order; defaults
/// `event=()`, `pc=0`, `vars={}`, `did=True`, `answer=False`, `waiting=False`, `buf=[]`,
/// `ints={}`, `once_key=()`, `acted=False`.
///
/// JSON (`:372-390`), keys in this order: `program_id` (content id in the canonical form),
/// `controller`, `host`, `host_seq`, `card_uid`, `kind`, `event`, `pc`, `vars` (`{name: [...]}`
/// sorted by name), `did`, `answer`, `waiting`, `buf`, `ints` (sorted by the rendered key, so
/// `__rest_as10` < `__rest_as2`), `once_key`, `acted`; the four bools as `true`/`false`.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub program: ProgramId,
    pub controller: u8,
    pub host: i32,
    pub host_seq: i32,
    pub card_uid: i32,
    pub kind: FrameKind,
    pub event: EventPayload,
    pub pc: u16,
    /// [`VarEntry`]s in `GameState::frame_vars`; their items in `frame_items`.
    pub vars: Span,
    pub did: bool,
    pub answer: bool,
    pub waiting: bool,
    /// Items in `GameState::frame_items`: Choose and Discard picks, ToDeck and Arrange orders.
    pub buf: Span,
    /// [`IntEntry`]s in `GameState::frame_ints`.
    pub ints: Span,
    /// `None` for Python's `()`.
    pub once_key: Option<OnceKey>,
    pub acted: bool,
}

/// One entry of `Frame.vars: dict[str, tuple[int, ...]]`. Every name the engine writes is in
/// the IR's `var_names` table, including the engine literals `card`, `pilot`, `unit`,
/// `__todeck`, `__play`, `looked` and `t` (PLAN §3.3); see `EngineVars`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VarEntry {
    pub var: VarId,
    /// Items in `GameState::frame_items`.
    pub items: Span,
}

/// A key of `Frame.ints: dict[str, int]` (Inv 06a §12.6).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IntKey {
    /// `"__hold_rules"` ([`crate::HOLD_RULES`], `interp.py:206-210`); deleted at 0.
    HoldRules,
    /// `ins.key` of `LoopInit`/`LoopNext` (`"__loop{n}"`, `interp.py:282-311`), whose name is
    /// `Registry::var_name`.
    Loop(VarId),
    /// `"__rest_i"` (`interp.py:669-701`).
    RestI,
    /// `f"__rest_as{i}"` (`interp.py:671`, `:683-702`).
    RestAs(u16),
    /// `"ex_used"` (`game.py:933`, `interp.py:951`).
    ExUsed,
}

/// One entry of `Frame.ints`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct IntEntry {
    pub key: IntKey,
    pub value: i32,
}

/// `HistoryEvent(kind, player, by, uid, def_id, source=-1)` (`engine/state.py:414-430`), built
/// only by `core.record` (`core.py:142-151`), which reads `def_id` at record time and does not
/// touch. JSON (`:425-426`): `[kind, player, by, uid, def_id, source]`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct HistoryEvent {
    pub kind: HistKind,
    /// Always an owner or a player index at the call sites (`core.py:728`, `:808`, ...).
    pub player: u8,
    pub by: i8,
    pub uid: i32,
    pub def_id: i32,
    pub source: i32,
}

/// An IR `HappenedThisTurn.kind` (`effects/dsl.py:523`, PLAN §4.2): a kind that some
/// `core.record` call site writes, or any other string. Python compares the strings
/// (`h.kind != c.kind`, `engine/view.py:919`), so an `Other` kind never matches a recorded
/// event. The generated node keeps the `StrId`; B2 decodes it with
/// `HistKindRef::decode(reg.str(kind), kind)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HistKindRef {
    Known(HistKind),
    Other(StrId),
}

impl HistKindRef {
    /// `text` is `Registry::str(id)`.
    #[must_use]
    pub fn decode(text: &str, id: StrId) -> Self {
        HistKind::from_value(text).map_or(Self::Other(id), Self::Known)
    }

    /// `h.kind == c.kind` for a recorded event of kind `kind`.
    #[must_use]
    pub fn matches(self, kind: HistKind) -> bool {
        self == Self::Known(kind)
    }
}

/// `Decision.prompt` (`engine/state.py:42`). Serialised as its text: an engine literal, the
/// AskMay prompt `ins.prompt or "you may"` (`interp.py:216`: `Ask` when `ins.prompt` is
/// non-empty, else `Engine(EnginePrompt::YouMay)`), or `f"choose {ins.var}"` (`interp.py:433`,
/// `:461`). `canon::from_canonical` recovers `Ask`/`Choose` from the waiting frame's instruction.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Prompt {
    Engine(EnginePrompt),
    /// `Registry::str(id)`.
    Ask(StrId),
    /// `"choose " + Registry::var_name(var)`.
    Choose(VarId),
}

/// `Decision(player, kind, options, prompt="", context=())` (`engine/state.py:35-68`) without
/// its options, which live in `GameState::pending_options`. `ctx(key)` is the first match,
/// default -1 (`:45-49`).
///
/// JSON (`:51-58`), keys in this order: `player`, `kind` (value), `options` (Action lists),
/// `prompt` (text; `ensure_ascii` in the digest writes `①` as `①`), `context`
/// (`[[key, value], ...]`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DecisionHdr {
    pub player: u8,
    pub kind: DecisionKind,
    pub prompt: Prompt,
    pub context: InlineVec<(CtxKey, i32), CONTEXT_CAPACITY>,
}

/// The pending decision of a state (`st.pending`): its header and its options in Python's
/// order. Returned by `GameState::pending`.
#[derive(Clone, Copy, Debug)]
pub struct DecisionView<'a> {
    pub hdr: &'a DecisionHdr,
    pub options: &'a [Action],
}

/// Read access to one frame's `vars` (`GameState::frame_vars`).
#[derive(Clone, Copy, Debug)]
#[allow(
    dead_code,
    reason = "read by workstream C's FrameVars accessors (docs/port/contracts.md)"
)]
pub struct FrameVars<'a> {
    pub(crate) entries: &'a [VarEntry],
    pub(crate) items: &'a [i32],
}

// ---------------------------------------------------------------------------------------------
// Game-level shared data and the state (engine/state.py:433-651)

/// Test-only behaviour overrides (PLAN §3.4): `force_link` lists the Unit card numbers that
/// count as linked with any paired card (`tests/cards/test_wp_gd05_a.py:101-113`). `None` in
/// production.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct TestOverrides {
    pub force_link: Box<[Box<str>]>,
}

/// The [`VarId`]s of the frame-variable names that engine and builtin-hook code writes or reads
/// literally (`game.py:931`, `:941`, `interp.py:808`, `:1409`, `:1416`, `builtins.py:16`,
/// `:68`), resolved once per registry with `Registry::var_id`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(
    dead_code,
    reason = "built and read by workstreams C and B2 (docs/port/contracts.md)"
)]
pub(crate) struct EngineVars {
    pub(crate) card: VarId,
    pub(crate) pilot: VarId,
    pub(crate) unit: VarId,
    pub(crate) todeck: VarId,
    pub(crate) play: VarId,
    pub(crate) looked: VarId,
    pub(crate) t: VarId,
}

/// Shared by every clone of one game (`Arc`), as Python shares the decklist tuples
/// (`state.py:561-562`) and the global registry.
#[allow(
    dead_code,
    reason = "built and read by workstream C (docs/port/contracts.md)"
)]
pub struct GameConst {
    pub(crate) reg: Arc<Registry>,
    pub(crate) db: &'static CardDb,
    /// `st.decklists`: per player, the sorted main-deck def_ids (`game.py:151`).
    pub(crate) decklists: [Box<[DefId]>; 2],
    /// `st.resource_decklists`, sorted (`game.py:152`).
    pub(crate) resource_decklists: [Box<[DefId]>; 2],
    pub(crate) turn_limit: i32,
    pub(crate) max_actions: i64,
    /// Exactly `listened_events` (`core.py:295-313`): the registry's `always_events`, plus
    /// `events[def]` and `events[unit_variant(def)]` for every def in the four decklists. Never a
    /// derived set (PLAN §4.2). Testkit helpers that change the decklists recompute it.
    pub(crate) listened: EvSet,
    /// The registry's global flag verbatim (`core.py:868`).
    pub(crate) hp_reduction_possible: bool,
    pub(crate) vars: EngineVars,
    pub(crate) test_overrides: Option<Arc<TestOverrides>>,
}

/// `zones[p][z]` (`state.py:479`) as one arena: list `i = 12 * p + z` is
/// `items[offs[i]..offs[i + 1]]` (PLAN §4.3). `zones[p][OUTSIDE]` is always empty.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[allow(
    dead_code,
    reason = "built and read by workstream C (docs/port/contracts.md)"
)]
pub struct ZoneLists {
    pub(crate) items: Vec<Uid>,
    pub(crate) offs: [u16; 2 * crate::N_ZONES + 1],
}

/// The complete state of one game (`engine/state.py:433-651`). Private to the crate (W-49):
/// other crates read it through the accessors frozen in docs/port/contracts.md. Inside the
/// crate the arenas change only through C's L1 helpers (`touch` included), and plain scalars and
/// the `cards`/`battles` records are written in place at the Python call sites.
///
/// `Clone` has value semantics (W-52): records, arenas and the pending decision are copied, and
/// `konst` and the cached view `Arc` are shared, as Python's `clone` shares the decklists and
/// `_derived` (`state.py:534-572`). `GameState` is `Send` but not `Sync` (the `RefCell`), so a
/// fork-join job receives its own clone (PLAN §4.6).
///
/// JSON (`state.py:575-611`): the 34 keys `seed, rng, cards, zones, turn, active,
/// first_player, setup_chooser, phase, step, pending, battles, next_battle_id,
/// next_lasting_serial, priority, passes, lasting, delayed, pending_triggers, batches, frames,
/// once_used, history, winner, end_reason, decklists, resource_decklists, redraws,
/// action_count, event_group, effect_hits, turn_limit, max_actions, version`, in this order;
/// `zones` is 2 x 12 lists including OUTSIDE; `once_used` sorted (the canonical form re-sorts
/// by `json.dumps` text); `effect_hits` as `[[uid, src], ...]` sorted by uid. `cache`,
/// `rules_checked` and `history_epoch` are not serialised (PLAN §4.7).
#[derive(Clone)]
#[allow(
    dead_code,
    reason = "built and read by workstreams C and B2 (docs/port/contracts.md)"
)]
pub struct GameState {
    /// Stored unmasked in Python (`state.py:476`); seeds are `u64` here.
    pub(crate) seed: u64,
    pub(crate) rng: SplitMix64,
    pub(crate) cards: Vec<CardInstance>,
    pub(crate) zones: ZoneLists,
    pub(crate) turn: i32,
    pub(crate) active: u8,
    pub(crate) first_player: u8,
    pub(crate) setup_chooser: u8,
    pub(crate) phase: Phase,
    pub(crate) step: Step,
    pub(crate) pending: Option<DecisionHdr>,
    pub(crate) pending_options: Vec<Action>,
    pub(crate) battles: Vec<Battle>,
    pub(crate) next_battle_id: i32,
    pub(crate) next_lasting_serial: i32,
    pub(crate) priority: u8,
    pub(crate) passes: i32,
    pub(crate) lasting: Vec<Lasting>,
    pub(crate) delayed: Vec<Lasting>,
    /// The `targets` arena of `lasting` and `delayed`.
    pub(crate) lasting_targets: Vec<(Uid, i32)>,
    /// `[batch 0 | ... | batch k | pending_triggers]` (PLAN §4.3).
    pub(crate) triggers: Vec<TriggerInst>,
    /// Start of each batch in `triggers`; empty batches are representable (Inv 05 §3.1).
    pub(crate) batch_starts: Vec<u32>,
    /// `pending_triggers = triggers[pending_start..]`.
    pub(crate) pending_start: u32,
    pub(crate) frames: Vec<Frame>,
    pub(crate) frame_vars: Vec<VarEntry>,
    pub(crate) frame_ints: Vec<IntEntry>,
    pub(crate) frame_items: Vec<i32>,
    /// A Python set used as an insertion-ordered set: `once_add` is the only writer and skips a
    /// key that is present (PLAN §4.3).
    pub(crate) once_used: Vec<OnceKey>,
    pub(crate) history: Vec<HistoryEvent>,
    /// `-1` is a draw (turn limit or simultaneous defeat).
    pub(crate) winner: Option<i8>,
    pub(crate) end_reason: Option<EndReason>,
    pub(crate) konst: Arc<GameConst>,
    pub(crate) redraws: [bool; 2],
    pub(crate) action_count: i64,
    pub(crate) event_group: i64,
    /// `dict[int, int]` uid -> source uid; overwriting keeps the first-insertion position.
    pub(crate) effect_hits: Vec<(Uid, Uid)>,
    /// Incremented only by `touch` (`state.py:529-531`).
    pub(crate) version: u64,
    /// Python `_derived` (PLAN §4.6): filled by `view::derived` on whatever state it is called
    /// on, cleared only by `touch` (through `get_mut`).
    pub(crate) cache: RefCell<Option<Arc<Derived>>>,
    /// Python `_rules_checked`, -1 initially (`state.py:511`, `core.py:861`).
    pub(crate) rules_checked: i64,
    /// Bumped whenever `history` is replaced (`game.py:190`, `:336`, `observe.py:80`, `:128`),
    /// standing in for the list identity that `runner/instrument.py:81` compares.
    pub(crate) history_epoch: u32,
}

// ---------------------------------------------------------------------------------------------
// Derived view (engine/view.py:27-106, :223-239, :314-333; PLAN §4.6)

/// `Ctx(controller, host=-1, card_uid=-1, vars=None, event=())` (`engine/view.py:27-50`).
/// Python's `card_uid = card_uid if card_uid != -1 else host` (`:42`) is applied by the
/// constructor (B2). `ctx_of(f)` shares `f.vars` (`interp.py:57-58`), which here is the frame's
/// index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Ctx {
    pub controller: u8,
    pub host: i32,
    pub card_uid: i32,
    pub vars: CtxVars,
    pub event: EventPayload,
}

/// Where a [`Ctx`] reads and writes its `vars`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CtxVars {
    /// A fresh empty dict (`vars=None`, `view.py:43`); writes are dropped, as Python drops them.
    Empty,
    /// `st.frames[i].vars`.
    Frame(u16),
}

/// `_view_key(st)` (`engine/view.py:223-229`): `(st.active, st.turn, len(st.battles),
/// (b.battle_id, b.target, b.ended) or (), len(st.history))`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ViewKey {
    pub active: u8,
    pub turn: i32,
    pub n_battles: u32,
    pub battle: Option<(i32, i32, bool)>,
    pub n_history: u32,
}

/// `RuleInst(rule, controller, source, key, aux=-1)` (`engine/view.py:53-63`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RuleInst {
    pub rule: RuleModId,
    pub controller: u8,
    pub source: i32,
    pub key: RuleKey,
    pub aux: i32,
}

/// `RuleInst.key`: `(aid, host)` for a constant ability (`view.py:431`, `:436`) or
/// `(-1, effect_key, serial)` for a lasting effect (`view.py:443-455`); it feeds the once keys
/// `(tag, *key, uid, zone_seq)` (`core.py:524`, `:561`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RuleKey {
    Constant { aid: AbilityId, host: Uid },
    Lasting { effect: ContRef, serial: i32 },
}

/// A uid's traits in a view: the printed traits of `def` followed by the granted ones in
/// `Derived::trait_pool` (`_base_view` `view.py:273`, `:279`; `TraitGrant` only appends,
/// `view.py:383-384`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TraitsRef {
    pub def: DefId,
    pub granted: Span,
}

/// `trigger_index(st, dv)` (`engine/view.py:314-333`): Triggered abilities by event, each list
/// in host order (active player first; BATTLE, BASE, PAIRED, HAND, TRASH; list order; per host
/// `dv.abilities[uid]` order). The event list of `ev` is `items[offs[ev]..offs[ev + 1]]`.
#[derive(Clone, PartialEq, Eq, Debug)]
#[allow(
    dead_code,
    reason = "built and read by workstream B2 (docs/port/contracts.md)"
)]
pub struct TriggerIndex {
    pub(crate) offs: [u32; N_EVENTS + 1],
    pub(crate) items: Box<[(Uid, AbilityId)]>,
}

/// `Derived` (`engine/view.py:66-106`), dense by uid (`len = cards.len()` at compute time).
/// Every accessor treats `uid >= len` as an absent key and returns Python's `.get` fallback
/// (PLAN §4.6). Immutable once built except `by_event`, which is filled once from the zones of
/// whichever state asks first (`view.py:317-333`; Inv 11 hazard H2). `Send + Sync`.
#[allow(
    dead_code,
    reason = "built and read by workstream B2 (docs/port/contracts.md)"
)]
pub struct Derived {
    /// The `_view_key` this view was computed for (Python's `()` default only for
    /// `Derived::empty()`, which is never cached).
    pub(crate) key: ViewKey,
    /// The `ap`/`hp`/`kw`/`traits` keys exist: the card was in BATTLE or BASE in the base view.
    pub(crate) present: Vec<bool>,
    /// Unclamped until `_finalize` (`view.py:477-483`).
    pub(crate) ap: Vec<i32>,
    pub(crate) hp: Vec<i32>,
    /// A key with an amount (`Some`) is present; a stacking keyword granted 0 is present.
    pub(crate) kw: Vec<KwAmounts>,
    pub(crate) traits: Vec<TraitsRef>,
    pub(crate) trait_pool: Vec<StrId>,
    /// Per uid in any zone, application order; empty = absent.
    pub(crate) rules: Vec<Span>,
    pub(crate) rule_pool: Vec<RuleInst>,
    /// `None` = key absent (only non-zero contributions create a key).
    pub(crate) cost_mod: Vec<Option<i32>>,
    pub(crate) level_mod: Vec<Option<i32>>,
    pub(crate) linked: Vec<bool>,
    /// Per host, zone-scan order, duplicates kept; empty = absent.
    pub(crate) abilities: Vec<Span>,
    pub(crate) ability_pool: Vec<AbilityId>,
    /// Insertion order of `view.abilities` (`view.py:245-311`), which the constant loop follows
    /// (`view.py:425`).
    pub(crate) hosts: Vec<Uid>,
    /// W-48: false when the fixpoint stopped at [`FIXPOINT_PASSES`] without converging.
    pub(crate) converged: bool,
    pub(crate) by_event: OnceLock<TriggerIndex>,
}

/// A uid's traits (`traits_of`, `view.py:560-562`): `printed` then `granted`.
#[derive(Clone, Copy, Debug)]
pub struct Traits<'a> {
    pub printed: &'a [Box<str>],
    pub granted: &'a [StrId],
}

/// `names_of(cd)` (`view.py:471-474`): `cd.names` then the registry's `NameAlias` aliases.
#[derive(Clone, Copy, Debug)]
pub struct Names<'a> {
    pub names: &'a [Box<str>],
    pub aliases: &'a [StrId],
}

/// A scratch list of uids replacing Python's temporary tuples (PLAN §10.3.4). Phase 5 may
/// replace it by a pooled buffer under the change protocol.
#[allow(
    dead_code,
    reason = "used by workstreams C and B2 (docs/port/contracts.md)"
)]
pub(crate) type UidBuf = Vec<Uid>;

// ---------------------------------------------------------------------------------------------
// Internal layer boundary (PLAN §10.3.4): keyword-only arguments and last-known information

/// The keyword-only arguments of `core.move` (`core.py:55-63`); all default to false.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[allow(
    dead_code,
    reason = "used by workstreams C and B2 (docs/port/contracts.md)"
)]
pub(crate) struct MoveOpts {
    pub(crate) bottom: bool,
    pub(crate) reveal: bool,
    pub(crate) rested: bool,
}

/// The keyword-only `source`, `battle`, `by` of the damage functions (`core.py:570-774`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(
    dead_code,
    reason = "used by workstreams C and B2 (docs/port/contracts.md)"
)]
pub(crate) struct DamageOpts {
    pub(crate) source: i32,
    pub(crate) battle: bool,
    pub(crate) by: i32,
}

/// The keyword arguments of `interp.push_frame` (`interp.py:61-71`): `card_uid=-1` falls back
/// to `host`; `vars` is copied into the new frame.
#[derive(Clone, Copy, Debug)]
#[allow(
    dead_code,
    reason = "used by workstreams C and B2 (docs/port/contracts.md)"
)]
pub(crate) struct PushFrame<'a> {
    pub(crate) controller: u8,
    pub(crate) host: i32,
    pub(crate) card_uid: i32,
    pub(crate) kind: FrameKind,
    pub(crate) event: EventPayload,
    pub(crate) vars: &'a [(VarId, &'a [i32])],
}

/// The keyword-only arguments of `interp.add_lasting` (`interp.py:1286-1298`): `player=-1`,
/// `uses=0`, `filters=()` (resolved to its filter set), `aux=-1`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(
    dead_code,
    reason = "used by workstreams C and B2 (docs/port/contracts.md)"
)]
pub(crate) struct LastingOpts {
    pub(crate) player: i8,
    pub(crate) uses: i32,
    pub(crate) filters: Option<FilterSetId>,
    pub(crate) aux: i32,
}

/// `dict[int, Snapshot]` of `ability_snapshot` (`core.py:485-500`): per uid in insertion order
/// (a repeated uid keeps its first position, the last value wins), its abilities and its
/// `zone_seq` at snapshot time (rules 13-2-8-2, 13-2-8-2-1).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[allow(
    dead_code,
    reason = "built and read by workstreams C and B2 (docs/port/contracts.md)"
)]
pub(crate) struct Lki {
    pub(crate) entries: Vec<LkiEntry>,
    pub(crate) abilities: Vec<AbilityId>,
}

/// One uid of an [`Lki`]: Python's `Snapshot(abilities, seq)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(
    dead_code,
    reason = "built and read by workstreams C and B2 (docs/port/contracts.md)"
)]
pub(crate) struct LkiEntry {
    pub(crate) uid: Uid,
    pub(crate) seq: i32,
    /// Into `Lki::abilities`.
    pub(crate) abilities: Span,
}

// ---------------------------------------------------------------------------------------------
// Public API types (PLAN §10.3.3)

/// The keyword arguments of `new_game(decks, seed, *, chooser=None, turn_limit=200,
/// max_actions=MAX_ACTIONS_DEFAULT, validate=True)` (`engine/game.py:107-116`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NewGameOptions {
    /// `None`: a seeded die roll decides (rule 6-2-1-4, `game.py:153`).
    pub chooser: Option<u8>,
    pub turn_limit: i32,
    pub max_actions: i64,
    pub validate: bool,
}

impl Default for NewGameOptions {
    /// Python's defaults.
    fn default() -> Self {
        Self {
            chooser: None,
            turn_limit: DEFAULT_TURN_LIMIT,
            max_actions: crate::MAX_ACTIONS_DEFAULT,
            validate: true,
        }
    }
}

/// `UnimplementedCardError(f"{e.card_number}: {e.error}")` (`effects/registry.py:19-20`,
/// `:143-147`): a game needs a card whose script is missing.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct UnimplementedCardError {
    pub card_number: Box<str>,
    pub reason: Box<str>,
}

/// Everything the engine raises besides illegal actions. `py_class` names the Python exception
/// class; `Display` (C) is the Python message where Python has one. Where Python's text embeds
/// CPython exception text, Rust matches the prefix only (PLAN A19). The `Overflow`, `Capacity`
/// and `NoProgress` variants have no Python counterpart (Python ints are unbounded, Python lists
/// grow, Python loops forever) and report as `EngineError`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum EngineError {
    /// `core.EngineError(message)` (`core.py:30-31`): an internal invariant.
    Internal(String),
    /// A Python `ValueError` raised inside the engine: `min()` of an empty `MinOf`
    /// (`view.py:878`), `cannot determinize: empty pool for hidden cards` (`observe.py:104`).
    Value(String),
    /// A Python `IndexError` on a path Python does not guard (for example
    /// `st.zones[p][Zone.DECK][0]` on an empty deck, `game.py:172`, `:199`).
    Index(String),
    /// A Python `KeyError` (for example `CardDB[number]`, `db.py:299-303`).
    Key(String),
    /// A failed Python `assert` statement (for example `game.py:262`).
    Assertion(String),
    /// `UnimplementedCardError` (`effects/registry.py:143-147`).
    Unimplemented(UnimplementedCardError),
    /// W-143: a REDIRECT_BATTLE_DAMAGE chain revisits a Unit (`core.py:576-599`); Python
    /// raises `RecursionError`.
    RedirectCycle { uids: Box<[Uid]> },
    /// A data-dependent integer left its Rust type (PLAN §4.1 item 6).
    Overflow { what: &'static str },
    /// An inline array or arena is full (PLAN §4.3 "Capacities").
    Capacity { what: &'static str, capacity: usize },
    /// W-47: `advance` ran [`NO_PROGRESS_STEPS`] iterations without a decision.
    NoProgress { steps: u64 },
}

impl EngineError {
    /// The Python exception class this error stands for (`type(exc).__name__`).
    #[must_use]
    pub const fn py_class(&self) -> &'static str {
        match self {
            Self::Internal(_)
            | Self::Overflow { .. }
            | Self::Capacity { .. }
            | Self::NoProgress { .. } => "EngineError",
            Self::Value(_) => "ValueError",
            Self::Index(_) => "IndexError",
            Self::Key(_) => "KeyError",
            Self::Assertion(_) => "AssertionError",
            Self::Unimplemented(_) => "UnimplementedCardError",
            Self::RedirectCycle { .. } => "RecursionError",
        }
    }
}

/// `IllegalActionError(ValueError)` (`engine/game.py:36-37`), raised by `apply`
/// (`game.py:783-786`): `"no decision pending"` or `f"illegal action {action} for
/// {dec.kind.value}"` (`str(action)`, `state.py:30-32`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IllegalActionError {
    NoDecision,
    Illegal { action: Action, kind: DecisionKind },
}

/// The errors of `apply` and `step`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ApplyError {
    Illegal(IllegalActionError),
    Engine(EngineError),
}

/// `DeckConstructionError(f"player {p}: " + "; ".join(problems))` (`engine/game.py:48-49`,
/// `:124-127`); `problems` are the texts of `deck_construction_problems` (`:58-100`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeckConstructionError {
    pub player: u8,
    pub problems: Vec<String>,
}

/// The errors of `new_game` (`engine/game.py:107-158`): construction problems (`:123-127`), an
/// unimplemented card (`R.require`, `:134`), or anything `advance` raises (`:157`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum NewGameError {
    Construction(DeckConstructionError),
    Unimplemented(UnimplementedCardError),
    Engine(EngineError),
}

/// A trace snapshot for `canon::from_canonical` (PLAN §4.8): the canonical state JSON plus the
/// side tables content ids cannot be inverted from, and Python's `_rules_checked`, which
/// `from_json` drops (`state.py:613-651`) but which decides `effect_hits.clear()`
/// (`core.py:857-892`). The decklists are inside `state`.
#[derive(Clone, PartialEq, Debug)]
pub struct StateSnapshot {
    pub state: serde_json::Value,
    /// `{content_id: node}` of the continuous effects the state names (lasting, delayed, the
    /// effect of `(-8 | -7, -1, effect, ...)` once keys), each a self-contained inline node
    /// (`{"_": class, field: value, ...}`, nested nodes inline, enums as their values; the
    /// `node_json` of `port/oracle/traces.py`), or `null` for none.
    pub continuous: serde_json::Value,
    /// `{content_id: [node, ...]}` of the filter sets the state names, each a list of inline
    /// filter nodes, or `null` for none.
    pub filter_sets: serde_json::Value,
    pub rules_checked: i64,
}

/// Why a snapshot could not be rebuilt into a state.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum CanonError {
    /// A key is missing or has the wrong JSON type; `path` is a JSON path (`$.cards[3][4]`).
    Shape { path: String, detail: String },
    /// A content id names no program, continuous effect or filter set of the registry;
    /// `kind` is `program`, `continuous` or `filter_set`.
    UnknownContent { kind: &'static str, id: String },
    /// A value is outside its type: a uid, zone, enum value string, var name or prompt text.
    Value { path: String, detail: String },
}

/// `InvariantViolation(AssertionError)` (`engine/invariants.py:22-23`): every message of
/// `check` in order; `Display` (C) is `"; ".join(errors[:10])` (`:108-109`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct InvariantViolation {
    pub errors: Vec<String>,
}

/// The errors of `invariants::check` (`engine/invariants.py:29-109`): the violations, or an
/// error of the `derived` call that the lethal-damage check makes (`:100`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum CheckError {
    Violation(InvariantViolation),
    Engine(EngineError),
}

/// `Counter((c.owner, db.base_def_id(c.def_id)) for non-token cards)`
/// (`engine/invariants.py:112-116`), sorted by key; compared for equality by `check`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[allow(
    dead_code,
    reason = "built and read by workstream C (docs/port/contracts.md)"
)]
pub struct CardMultiset(pub(crate) Box<[((u8, DefId), u32)]>);

/// `information_set_key(st, observer)` (`engine/observe.py:134-150`): equality and hashing
/// only (its Python users compare keys, `tests/ai/test_information_set.py:61`). Python compares
/// `repr(pending.to_json())`; equal headers and options are equal decisions because every
/// prompt of one program has one [`Prompt`] form.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[allow(dead_code, reason = "built by workstream C (docs/port/contracts.md)")]
pub struct InfoSetKey {
    pub(crate) cards: Box<[InfoCard]>,
    pub(crate) zones: ZoneLists,
    pub(crate) turn: i32,
    pub(crate) active: u8,
    pub(crate) step: Step,
    pub(crate) pending: Option<(DecisionHdr, Box<[Action]>)>,
}

/// One card of an [`InfoSetKey`]: `(uid, -1 if hidden else def_id, owner, int(zone), rested,
/// damage, pair)` (`observe.py:136-147`); the uid is the index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[allow(dead_code, reason = "built by workstream C (docs/port/contracts.md)")]
pub(crate) struct InfoCard {
    pub(crate) def_id: i32,
    pub(crate) owner: u8,
    pub(crate) zone: Zone,
    pub(crate) rested: bool,
    pub(crate) damage: i32,
    pub(crate) pair: i32,
}

// ---------------------------------------------------------------------------------------------
// Testkit (feature `testkit`, PLAN A18; testkit.py)

/// The keyword arguments of `Scenario(*, active=0, turn=3, seed=0, deck_size=20)`
/// (`testkit.py:38-40`).
#[cfg(feature = "testkit")]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ScenarioOptions {
    pub active: u8,
    pub turn: i32,
    pub seed: u64,
    pub deck_size: usize,
}

#[cfg(feature = "testkit")]
impl Default for ScenarioOptions {
    /// Python's defaults.
    fn default() -> Self {
        Self {
            active: 0,
            turn: 3,
            seed: 0,
            deck_size: 20,
        }
    }
}

/// The keyword arguments of `Scenario.add(player, number, zone=BATTLE, *, rested=False,
/// damage=0, pilot=None, deployed_this_turn=False, known=True)` (`testkit.py:52-63`).
#[cfg(feature = "testkit")]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AddOptions<'a> {
    pub rested: bool,
    pub damage: i32,
    /// A Pilot card number to pair with the added Unit.
    pub pilot: Option<&'a str>,
    pub deployed_this_turn: bool,
    pub known: bool,
}

#[cfg(feature = "testkit")]
impl Default for AddOptions<'_> {
    /// Python's defaults.
    fn default() -> Self {
        Self {
            rested: false,
            damage: 0,
            pilot: None,
            deployed_this_turn: false,
            known: true,
        }
    }
}

/// `Scenario` (`testkit.py:35-182`): builds a mid-game state directly, bypassing setup.
#[cfg(feature = "testkit")]
#[allow(
    dead_code,
    reason = "built and read by workstream C's testkit (docs/port/contracts.md)"
)]
pub struct Scenario {
    /// `self.st`, created as `GameState(seed)` with `turn`, `active` and `first_player` set
    /// (`testkit.py:41-44`).
    pub(crate) st: GameState,
    pub(crate) deck_size: usize,
    /// `self._decks`: card numbers, first = top (`testkit.py:108-111`).
    pub(crate) decks: [Vec<Box<str>>; 2],
    pub(crate) custom_deck: [bool; 2],
    pub(crate) started: bool,
}

/// The keyword arguments of `interp.push_frame(st, program_id, *, controller, host,
/// card_uid=-1, kind, event=(), vars=None)` (`engine/interp.py:61-71`) for
/// `testkit::engine::push_frame`, the public twin of the internal `PushFrame`: `card_uid` -1
/// falls back to `host`, and `vars` is copied into the new frame.
#[cfg(feature = "testkit")]
#[derive(Clone, Copy, Debug)]
pub struct FrameArgs<'a> {
    pub controller: u8,
    pub host: i32,
    pub card_uid: i32,
    pub kind: FrameKind,
    pub event: EventPayload,
    pub vars: &'a [(VarId, &'a [i32])],
}

/// The keyword-only arguments of `interp.add_lasting(..., *, player=-1, uses=0, filters=(),
/// aux=-1)` (`engine/interp.py:1293-1297`) for `testkit::engine::add_lasting`. `filters` are
/// the test's filter nodes; a non-empty tuple is resolved with `Registry::filters_key`
/// (`interp.py:1314`).
#[cfg(feature = "testkit")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LastingArgs<'a> {
    pub player: i8,
    pub uses: i32,
    pub filters: &'a [gcg_effects::FilterId],
    pub aux: i32,
}

#[cfg(feature = "testkit")]
impl Default for LastingArgs<'_> {
    /// Python's defaults.
    fn default() -> Self {
        Self {
            player: -1,
            uses: 0,
            filters: &[],
            aux: crate::NO_ARG,
        }
    }
}

/// What a testkit helper raises: Python's `AssertionError` texts (`testkit.py:228-234` and the
/// helpers that call `_fail`), a card lookup failure, or an engine error.
#[cfg(feature = "testkit")]
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TestkitError {
    Assertion(String),
    /// `ValueError(f"not enough vanilla {color} Units for a legal deck")` (`testkit.py:197-198`).
    Value(String),
    /// `CardDB[number]` raised `KeyError` (`cards/db.py:299-303`).
    UnknownCard(Box<str>),
    Apply(ApplyError),
    Engine(EngineError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send<T: Send>() {}
    fn send_sync<T: Send + Sync>() {}

    #[test]
    fn states_are_send_and_views_are_shareable() {
        send::<GameState>();
        send_sync::<Derived>();
        send_sync::<GameConst>();
    }

    #[test]
    fn records_stay_copy_and_small() {
        fn copy<T: Copy>() {}
        copy::<CardInstance>();
        copy::<TriggerInst>();
        copy::<Frame>();
        copy::<Lasting>();
        copy::<DecisionHdr>();
        assert!(size_of::<CardInstance>() <= 24);
        assert!(size_of::<Action>() <= 16);
        assert!(size_of::<EventPayload>() <= 128);
    }

    #[test]
    fn once_keys_order_element_wise_with_shorter_prefix_first() {
        let key = |xs: &[i32]| {
            let mut k = OnceKey::default();
            for &x in xs {
                k.0.try_push(OnceElem::Int(x)).unwrap();
            }
            k
        };
        let mut keys = [
            key(&[1, 2, 3]),
            key(&[0, 5]),
            key(&[1]),
            key(&[-8, 3, 4]),
            key(&[1, 2]),
        ];
        keys.sort();
        assert_eq!(
            keys,
            [
                key(&[-8, 3, 4]),
                key(&[0, 5]),
                key(&[1]),
                key(&[1, 2]),
                key(&[1, 2, 3])
            ]
        );
    }

    #[test]
    fn defaults_are_pythons() {
        let g = NewGameOptions::default();
        assert_eq!(
            (g.chooser, g.turn_limit, g.max_actions, g.validate),
            (None, 200, 20000, true)
        );
        assert_eq!(
            Action::of(ActionKind::EndMain),
            Action::new(ActionKind::EndMain, -1, -1, -1)
        );
        assert_eq!(
            EngineError::RedirectCycle { uids: Box::new([]) }.py_class(),
            "RecursionError"
        );
    }

    #[cfg(feature = "testkit")]
    #[test]
    fn testkit_defaults_are_pythons() {
        let l = LastingArgs::default();
        assert_eq!((l.player, l.uses, l.filters.len(), l.aux), (-1, 0, 0, -1));
        let s = ScenarioOptions::default();
        assert_eq!((s.active, s.turn, s.seed, s.deck_size), (0, 3, 0, 20));
        let a = AddOptions::default();
        assert_eq!(
            (a.rested, a.damage, a.pilot, a.deployed_this_turn, a.known),
            (false, 0, None, false, true)
        );
    }

    #[test]
    fn only_recorded_history_kinds_match() {
        let destroyed = HistKindRef::decode("destroyed", StrId(3));
        assert_eq!(destroyed, HistKindRef::Known(HistKind::Destroyed));
        assert!(destroyed.matches(HistKind::Destroyed));
        assert!(!destroyed.matches(HistKind::Deployed));
        let other = HistKindRef::decode("never_recorded", StrId(4));
        assert_eq!(other, HistKindRef::Other(StrId(4)));
        assert!(HistKind::ALL.iter().all(|&k| !other.matches(k)));
    }
}
