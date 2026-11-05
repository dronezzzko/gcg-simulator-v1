//! Contract types of `gcg-effects` (docs/dev/PLAN.md §3.3, §5.1-§5.3, §5.6, §10.3.2), owned
//! by the lead (§10.1 S). The DSL node types themselves are generated into `dsl.rs` by
//! `oracle/gen_dsl.py`.
//!
//! Workstream B1 implements the functions frozen in docs/dev/contracts.md (gcg-effects
//! section) with real bodies; only the lead edits this file (§10.6 change protocol). Fields
//! marked `pub(crate)` are the lead's starting layout of an opaque type: B1 may change them in
//! its landing diff, because no other crate can observe them.
//!
//! Python sources are cited relative to `src/gcg_sim/`.

use gcg_data::{CardDb, DefId};

use crate::dsl::{Ev, Kw, Node, P, StepHook};

/// The only IR format this build reads (`ir_version`, PLAN §3.4). Any other value is
/// [`IrError::Version`], naming the regeneration command.
pub const SUPPORTED_IR_VERSION: u32 = 1;

/// The command that regenerates the IR (PLAN §3.4), named by IR errors.
pub const IR_REGENERATE_COMMAND: &str = "uv run python oracle/export_ir.py --write";

// ---------------------------------------------------------------------------------------------
// Handles. The derived `Ord` of ids is arbitrary but total; it is used only to sort for
// equality tests (PLAN §4.2 `ContRef`), never to reproduce a Python order.

/// Index into the hash-consed node arena (`Registry::nodes`). Two nodes have the same id iff
/// their CPython `repr` is equal, which the exporter proves equals Python `==` (PLAN §3.3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId(pub u32);

/// Index into `Registry::programs` (Python's `program_id`, `effects/registry.py:115-122`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ProgramId(pub u16);

/// Python's ability id `aid` (`effects/registry.py:223-284`); observable in once keys,
/// `ability_key`, rule keys and the ACTIVATE action, so it is loaded, never renumbered.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AbilityId(pub u16);

/// Index into the IR string table (card numbers, names, traits, labels, `AskMay` prompts, keys).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StrId(pub u32);

/// Index into the IR `var_names` table (frame-variable and loop-key names, PLAN §3.3).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VarId(pub u16);

/// Index into the IR `filter_sets` (Python's `filters_key`, `effects/registry.py:132-138`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FilterSetId(pub u16);

/// `sha256(repr(x).encode()).hexdigest()[:16]` (`runner/digest.py:26-27`): the 16 lowercase
/// hex characters as ASCII bytes. Only `ContentId::from_hex` and the IR loader construct one,
/// so the bytes are always ASCII and `ContentId::as_str` cannot fail.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[allow(
    dead_code,
    reason = "read by workstream B1's ContentId::as_str and IR loader (docs/dev/contracts.md)"
)]
pub struct ContentId(pub(crate) [u8; 16]);

/// Generates a node handle of one alias kind, checked against `dsl::NodeKind::<KIND>` when the
/// IR loads (PLAN §5.1). Only this crate constructs one.
macro_rules! node_handle {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        pub struct $name(pub(crate) NodeId);

        impl $name {
            /// The underlying arena index.
            #[must_use]
            pub const fn node(self) -> NodeId {
                self.0
            }
        }
    };
}

node_handle!(
    /// A node of `type Filter` (`effects/dsl.py:362`).
    FilterId
);
node_handle!(
    /// A node of `type Ref` (`effects/dsl.py:462`).
    RefId
);
node_handle!(
    /// A `Sel` node (`effects/dsl.py:393`).
    SelId
);
node_handle!(
    /// A node of `type Cond` (`effects/dsl.py:562`).
    CondId
);
node_handle!(
    /// A node of `type Step` (`effects/dsl.py:1197`).
    StepId
);
node_handle!(
    /// A node of `type Continuous` (`effects/dsl.py:686`).
    ContinuousId
);
node_handle!(
    /// A `Trigger` node (`effects/dsl.py:730`).
    TriggerId
);
node_handle!(
    /// A `RuleMod` node (`effects/dsl.py:631`).
    RuleModId
);
node_handle!(
    /// A node of `type Cost` (`effects/dsl.py:804`).
    CostId
);
node_handle!(
    /// A node of `type Ability` (`effects/dsl.py:1373`); distinct from the ability id
    /// [`AbilityId`].
    AbilityNodeId
);

/// A `Value` field (`type Value = int | ...`, `effects/dsl.py:180`): a literal or a node.
/// The two stay distinct because `eff.ap != 0` is true for every node (`engine/view.py:360-361`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ValueRef {
    Lit(i64),
    Node(NodeId),
}

/// One value of a custom hook's `params` (`tuple[tuple[str, object], ...]`). The data holds
/// ints, strs and `Cond`/`Sel` nodes only; the exporter asserts it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ParamValue {
    Int(i64),
    Str(StrId),
    Node(NodeId),
}

// ---------------------------------------------------------------------------------------------
// Programs, abilities, cards (effects/program.py, effects/registry.py)

/// `Program(instrs, label)` (`effects/program.py:90-93`), exactly as exported: the layout is
/// Python's `flatten` output (`program.py:96-181`), which `choice_group`
/// (`interp.py:391-404`), `targets_available` (`:235-260`) and hook #33
/// (`wp_st12_14.py:91-93`) read.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Program {
    /// The first registrant's label (`registry.py:115-122`); part of `repr` and so of the
    /// content id.
    pub label: StrId,
    pub content_id: ContentId,
    pub instrs: Box<[NodeId]>,
}

/// `AbilityEntry` (`effects/registry.py:23-33`); index = `aid`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AbilityEntry {
    pub aid: AbilityId,
    pub card_number: StrId,
    pub def_id: DefId,
    pub index: u16,
    pub unit_text: bool,
    pub ability: AbilityNodeId,
    /// `program_id`; `None` for -1 (no steps: keywords, constants).
    pub program: Option<ProgramId>,
    /// `cost_program_id`; `None` for -1.
    pub cost_program: Option<ProgramId>,
    /// Set for `Triggered` only.
    pub trigger_event: Option<Ev>,
}

/// `CardScript.source` as the registry reports it (`effects/scripts.py:12-16`,
/// `effects/bindings/__init__.py:9`, `effects/compiler/__init__.py:56,67`).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ScriptSource {
    Binding = 0,
    Compiled = 1,
    Vanilla = 2,
}

impl ScriptSource {
    pub const ALL: [Self; 3] = [Self::Binding, Self::Compiled, Self::Vanilla];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Binding => "binding",
            Self::Compiled => "compiled",
            Self::Vanilla => "vanilla",
        }
    }
}

/// A set of [`Ev`] (bit `i` = the member with discriminant `i`; 33 members). Python's
/// frozensets of events (`events_by_def`, `always_events`, `listened`) are used for membership
/// only.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct EvSet(pub u64);

impl EvSet {
    pub const EMPTY: Self = Self(0);

    #[must_use]
    pub const fn contains(self, ev: Ev) -> bool {
        self.0 & (1 << ev as u32) != 0
    }

    #[must_use]
    pub const fn with(self, ev: Ev) -> Self {
        Self(self.0 | (1 << ev as u32))
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Keyword amounts by [`Kw`] discriminant: `Some(n)` where Python's dict has the key (a
/// 0-amount keyword counts as present, `wp_gd04_a.py:62-66`). Used for `printed_keywords`
/// (`effects/registry.py:89-113`) and per-uid derived keywords (PLAN §4.6).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct KwAmounts(pub [Option<i16>; 7]);

impl KwAmounts {
    #[must_use]
    pub const fn get(&self, kw: Kw) -> Option<i16> {
        self.0[kw as usize]
    }
}

/// `CardEntry` (`effects/registry.py:36-54`) as exported (PLAN §3.3 `cards`); index = def_id.
/// `script` is never `None` for a registry that built (W-69 fails the build instead).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CardEntry {
    pub def_id: DefId,
    pub source: ScriptSource,
    /// The binding module's short name (`wp_gd01_a`), when bound.
    pub binding_module: Option<StrId>,
    /// The `CardScript` node.
    pub script: Option<NodeId>,
    pub own: Box<[AbilityId]>,
    pub unit: Box<[AbilityId]>,
    /// The last own `Command` (`registry.py:257-261`).
    pub command_aid: Option<AbilityId>,
    /// The last own `Burst` (`registry.py:257-261`).
    pub burst_aid: Option<AbilityId>,
    pub has_burst: bool,
    /// `timings` (a frozenset, `registry.py:189-200`), stored sorted.
    pub timings: Box<[StrId]>,
    pub field_entries: Box<[AbilityId]>,
    pub hand_entries: Box<[AbilityId]>,
    pub trash_entries: Box<[AbilityId]>,
    pub paired_entries: Box<[AbilityId]>,
    pub unit_entries: Box<[AbilityId]>,
    pub gated: bool,
    /// `events_by_def[def_id]` (`registry.py:356-397`).
    pub events: EvSet,
    /// `printed_keywords(def_id)` (`registry.py:89-113`), exported per card so no cache is
    /// needed; fragments may mark entries "absent" (PLAN §3.4 test overrides).
    pub printed_keywords: KwAmounts,
    /// `(n, DelayedTrigger node, program)` in `core._walk_delayed` order (PLAN §3.3).
    pub delayed: Box<[(u16, NodeId, ProgramId)]>,
    /// Effect-template id (§12.11); never read by the engine.
    pub template: Option<StrId>,
}

/// The engine's keyword programs (`effects/registry.py:58-65`; ids 0, 1, 2 in the IR's
/// `keyword_programs`).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum KeywordProgram {
    /// `keyword:Repair`, `REPAIR_STEPS`.
    Repair = 0,
    /// `keyword:Breach`, `BREACH_STEPS`.
    Breach = 1,
    /// `keyword:Support`, `SUPPORT_STEPS`.
    Support = 2,
}

impl KeywordProgram {
    pub const ALL: [Self; 3] = [Self::Repair, Self::Breach, Self::Support];

    /// The key in the IR's `keyword_programs` table.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Repair => "repair",
            Self::Breach => "breach",
            Self::Support => "support",
        }
    }
}

/// The engine's system programs (`engine/interp.py:1581-1594`). Python registers them lazily
/// in first-use order; Rust loads the IR's fixed ids (623, 624, 625) with the labels
/// `system:deploy`, `system:pair`, `system:hand_limit` (PLAN §5.2).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SystemProgram {
    /// `(DeployCard(Var("card")),)`.
    Deploy = 0,
    /// `(Pair(Var("pilot"), Var("unit")),)`.
    Pair = 1,
    /// `(Discard(count=Sum((HandSize(), -10))),)`.
    HandLimit = 2,
}

impl SystemProgram {
    pub const ALL: [Self; 3] = [Self::Deploy, Self::Pair, Self::HandLimit];

    /// The key in the IR's `system_programs` table.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deploy => "deploy",
            Self::Pair => "pair",
            Self::HandLimit => "hand_limit",
        }
    }

    /// The program label, part of its content id.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Deploy => "system:deploy",
            Self::Pair => "system:pair",
            Self::HandLimit => "system:hand_limit",
        }
    }
}

/// One `filter_sets` entry of the IR: a filter tuple and its content id.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct FilterSet {
    pub filters: Box<[FilterId]>,
    pub content_id: ContentId,
}

// ---------------------------------------------------------------------------------------------
// Continuous-effect identity (PLAN §5.6)

/// A continuous effect value. Replaces `Registry.continuous[effect_key]`
/// (`effects/registry.py:124-130`), whose first-use numbering is process-local.
///
/// Equality of `ContRef` values equals Python's `cont_key` interning (PLAN §5.6): value-shaped
/// effects are always value variants, whatever their origin (an IR literal, `_frozen`
/// `interp.py:1276-1283`, a hook `wp_gd03_a.py:111` and `wp_gd04_a.py:69-106`, or `_h_delayed`
/// `interp.py:1525-1557`); every other continuous node is `Node` of its hash-consed IR node.
/// The derived `Ord` is arbitrary but total (over `NodeId`, `StrId` and `Kw` declaration
/// order); it exists only so that `_references` and `_fingerprint` tuples can be sorted
/// (`ai/actions.py:308-329`, `:360`), which Python compares only for equality in the merge set
/// (`:386-392`). Python's integer order is never reproduced.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ContRef {
    /// Any `StatMod` with int fields (frozen, or an IR literal).
    StatMod { ap: i32, hp: i32 },
    /// Any `KeywordGrant` with an int amount.
    KeywordGrant { kw: Kw, amount: i32 },
    /// The delayed-trigger key `AbilityGrant(card_number, n)` (`interp.py:1549`) or a literal.
    AbilityGrant { card: StrId, index: i32 },
    /// Every other `Continuous` node.
    Node(NodeId),
}

/// Typed read access to a continuous effect for `_apply_cont` (`engine/view.py:347-406`):
/// value variants of [`ContRef`] read as literals.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContView<'a> {
    StatMod {
        ap: ValueRef,
        hp: ValueRef,
    },
    KeywordGrant {
        keyword: Kw,
        amount: ValueRef,
    },
    TraitGrant {
        traits: &'a [StrId],
    },
    RuleGrant {
        rule: RuleModId,
    },
    AbilityGrant {
        card_number: StrId,
        ability_index: i64,
    },
    CostMod {
        cost: ValueRef,
        level: ValueRef,
    },
}

// ---------------------------------------------------------------------------------------------
// Custom hooks (PLAN §5.3). The names are the generated `dsl::{StepHook,
// CondHook, FilterHook, ValueHook}`; the typed parameters below are decoded once at load from
// `dict(params)` (last key wins, `interp.py:1573`, `view.py:781,882,933`), and the raw
// `params` stay in the node for `repr`. #6 and #7 keep separate names (their reprs differ) and
// share one body (W-64).

/// Typed `params` of a `CustomStep` (13 hooks).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StepParams {
    /// #1 `return_looked_bottom` (`effects/builtins.py:13`).
    ReturnLookedBottom,
    /// #2 `deploy_ex_base` (`effects/builtins.py:32`).
    DeployExBase,
    /// #5 `support_used` (`effects/builtins.py:65`).
    SupportUsed,
    /// #6 `wp_eb01_a_look_own_top` (`wp_eb01_a.py:61`): `d.P(str(params["player"]))`,
    /// `str(params["var"])`.
    EB01aLookOwnTop { player: P, var: VarId },
    /// #7 `wp_eb01_b_look_top_own` (`wp_eb01_b.py:240`): same parameters as #6.
    EB01bLookTopOwn { player: P, var: VarId },
    /// #9 `wp_gd02_b_pay_deploy_cost` (`wp_gd02_b.py:54`): `str(params["var"])`.
    GD02bPayDeployCost { var: VarId },
    /// #13 `wp_gd03_a_pay_cost_of` (`wp_gd03_a.py:92`): `params["var"]` (a str).
    GD03aPayCostOf { var: VarId },
    /// #14 `wp_gd03_a_ap_minus_count` (`wp_gd03_a.py:111`): `params["var"]` (a str),
    /// `params["sel"]` (a `Sel`).
    GD03aApMinusCount { var: VarId, sel: SelId },
    /// #20 `wp_gd04_a_copy_keywords` (`wp_gd04_a.py:69`): `str(params["var"])`.
    GD04aCopyKeywords { var: VarId },
    /// #21 `wp_gd04_a_ap_down_by_named_trash_units` (`wp_gd04_a.py:84`):
    /// `str(params["name"])`, `str(params["var"])`.
    GD04aApDownByNamedTrashUnits { name: StrId, var: VarId },
    /// #26 `wp_gd05_a_use_once` (`wp_gd05_a.py:116`): `_int_param(params, "tag")`.
    GD05aUseOnce { tag: i64 },
    /// #28 `wp_gd05_b_destroy_first_shield_area_cards` (`wp_gd05_b.py:70`):
    /// `params["count"]` (an int).
    GD05bDestroyFirstShieldAreaCards { count: i64 },
    /// #29 `wp_gd05_b_resolving_command_to_trash` (`wp_gd05_b.py:90`).
    GD05bResolvingCommandToTrash,
}

impl StepParams {
    /// The hook these parameters belong to.
    #[must_use]
    pub const fn hook(&self) -> StepHook {
        match self {
            Self::ReturnLookedBottom => StepHook::ReturnLookedBottom,
            Self::DeployExBase => StepHook::DeployExBase,
            Self::SupportUsed => StepHook::SupportUsed,
            Self::EB01aLookOwnTop { .. } => StepHook::EB01aLookOwnTop,
            Self::EB01bLookTopOwn { .. } => StepHook::EB01bLookTopOwn,
            Self::GD02bPayDeployCost { .. } => StepHook::GD02bPayDeployCost,
            Self::GD03aPayCostOf { .. } => StepHook::GD03aPayCostOf,
            Self::GD03aApMinusCount { .. } => StepHook::GD03aApMinusCount,
            Self::GD04aCopyKeywords { .. } => StepHook::GD04aCopyKeywords,
            Self::GD04aApDownByNamedTrashUnits { .. } => StepHook::GD04aApDownByNamedTrashUnits,
            Self::GD05aUseOnce { .. } => StepHook::GD05aUseOnce,
            Self::GD05bDestroyFirstShieldAreaCards { .. } => {
                StepHook::GD05bDestroyFirstShieldAreaCards
            }
            Self::GD05bResolvingCommandToTrash => StepHook::GD05bResolvingCommandToTrash,
        }
    }
}

/// Typed `params` of a `CustomCond` (7 hooks).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CondParams {
    /// #4 `attack_target_damaged` (`effects/builtins.py:54`).
    AttackTargetDamaged,
    /// #8 `wp_gd01_b_destroyed_with_damage` (`wp_gd01_b.py:117`).
    GD01bDestroyedWithDamage,
    /// #23 `wp_gd04_b_played_with_ex` (`wp_gd04_b.py:52`).
    GD04bPlayedWithEx,
    /// #25 `wp_gd05_a_once_free` (`wp_gd05_a.py:110`): `_int_param(params, "tag")`.
    GD05aOnceFree { tag: i64 },
    /// #27 `wp_gd05_a_destroyed_by_effect` (`wp_gd05_a.py:122`): `by_you` is
    /// `params.get("by") == "you"` (the hook tests `!= "you"`).
    GD05aDestroyedByEffect { by_you: bool },
    /// #31 `wp_gd05_b_played_with_ex` (`wp_gd05_b.py:115`).
    GD05bPlayedWithEx,
    /// #33 `wp_st12_14_destroyed_by_effect_damage` (`wp_st12_14.py:83`).
    ST12DestroyedByEffectDamage,
}

/// Typed `params` of a `CustomFilter` (9 hooks).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FilterParams {
    /// #3 `is_attack_target` (`effects/builtins.py:46`).
    IsAttackTarget,
    /// #11 `wp_gd03_a_has_destroyed_effect` (`wp_gd03_a.py:62`).
    GD03aHasDestroyedEffect,
    /// #12 `wp_gd03_a_lowest_hp` (`wp_gd03_a.py:79`).
    GD03aLowestHp,
    /// #15 `wp_gd03_b_cond_holds` (`wp_gd03_b.py:64`): `params["cond"]` (a `Cond`).
    GD03bCondHolds { cond: CondId },
    /// #16 `wp_gd03_b_battling_enemy_unit` (`wp_gd03_b.py:72`).
    GD03bBattlingEnemyUnit,
    /// #18 `wp_gd04_a_pilot_capable` (`wp_gd04_a.py:50`).
    GD04aPilotCapable,
    /// #19 `wp_gd04_a_has_printed_keyword` (`wp_gd04_a.py:62`).
    GD04aHasPrintedKeyword,
    /// #22 `wp_gd04_a_has_partner_in_trash` (`wp_gd04_a.py:112`): `str(params["trait"])`.
    GD04aHasPartnerInTrash { r#trait: StrId },
    /// #30 `wp_gd05_b_can_pair` (`wp_gd05_b.py:106`).
    GD05bCanPair,
}

/// Typed `params` of a `CustomValue` (5 hooks).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ValueParams {
    /// #10 `wp_gd03_a_ap_per` (`wp_gd03_a.py:52`): `params["per"]` (an int).
    GD03aApPer { per: i64 },
    /// #17 `wp_gd03_b_unique_names` (`wp_gd03_b.py:96`): `params["sel"]` (a `Sel`).
    GD03bUniqueNames { sel: SelId },
    /// #24 `wp_gd05_a_lowest_enemy_unit_level` (`wp_gd05_a.py:97`).
    GD05aLowestEnemyUnitLevel,
    /// #32 `wp_st05_08_highest_enemy_unit_lv` (`wp_st05_08.py:46`).
    ST05HighestEnemyUnitLv,
    /// #34 `wp_st12_14_lowest_rested_enemy_lv` (`wp_st12_14.py:96`).
    ST12LowestRestedEnemyLv,
}

/// The decoded parameters of one `Custom*` node, kept by the registry next to the node.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HookParams {
    Step(StepParams),
    Cond(CondParams),
    Filter(FilterParams),
    Value(ValueParams),
}

// ---------------------------------------------------------------------------------------------
// Registry (effects/registry.py) and its loading errors

/// The loaded effect registry: the IR of PLAN §3.3 in typed form. Immutable once built;
/// `with_fragment` builds a new one with a new `generation` (W-33).
///
/// Layout (B1's proposal, authorised by the lead at the registry integration): the decoded IR
/// tables, which `Registry::view` and the golden writer read, plus the card database the IR
/// was checked against (`Registry::from_ir`).
pub struct Registry {
    /// Changes on every `with_fragment`; AI memo tables key on it (W-33, PLAN §5.2).
    pub(crate) generation: u32,
    pub(crate) db: &'static CardDb,
    /// The decoded IR (strings, nodes, programs, abilities, cards, continuous effects, filter
    /// sets, aliases, registry-global values, typed hook parameters, golden card texts).
    pub(crate) tables: crate::ir::Tables,
}

/// A self-contained node of a trace snapshot's side table (PLAN §4.8; the `node_json` form of
/// `oracle/traces.py`: `{"_": class, field: value, ...}` with nested nodes inline and enums
/// as their values), resolved against a registry by `Registry::decode_inline`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum InlineNode {
    /// The arena node whose CPython `repr` equals the inline node's.
    Arena(NodeId),
    /// A node no arena node equals (a runtime value such as a frozen `StatMod`); every node
    /// nested in it is an arena node.
    Detached(Node),
}

/// Why an IR or an IR fragment could not be loaded. Every variant names enough to act on:
/// the regeneration command for stale IR, the card number for an unknown class or hook (W-69:
/// no silent unimplemented card).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IrError {
    /// The text is not JSON.
    Json { message: String },
    /// `ir_version` is not [`SUPPORTED_IR_VERSION`].
    Version { found: i64 },
    /// The IR's `schema` or `enums` differ from `dsl::CLASSES` / `dsl::ENUMS` (PLAN §3.4):
    /// the first difference.
    Schema { detail: String },
    /// A node class outside `dsl::NodeKind`.
    UnknownClass {
        class: String,
        card_number: Option<String>,
    },
    /// A hook name outside the 34-name table; `kind` is `step`, `cond`, `filter` or `value`.
    UnknownHook {
        kind: &'static str,
        name: String,
        card_number: Option<String>,
    },
    /// A table, node or field has the wrong JSON shape, kind or range; `path` is a JSON path.
    Malformed { path: String, detail: String },
    /// The IR was exported from other card data than `gcg-data` embeds (`db_fingerprints`,
    /// def_id order, `data_lock_sha256`).
    DataMismatch {
        which: &'static str,
        expected: String,
        found: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ev_sets_hold_all_33_events() {
        let mut all = EvSet::EMPTY;
        for ev in Ev::ALL {
            assert!(!all.contains(ev));
            all = all.with(ev);
            assert!(all.contains(ev));
        }
        assert_eq!(all.0.count_ones(), 33);
        assert_eq!(EvSet::EMPTY.union(all), all);
    }

    #[test]
    fn program_tables_name_pythons_keys() {
        let kw: Vec<&str> = KeywordProgram::ALL
            .into_iter()
            .map(KeywordProgram::as_str)
            .collect();
        assert_eq!(kw, ["repair", "breach", "support"]);
        let sys: Vec<&str> = SystemProgram::ALL
            .into_iter()
            .map(SystemProgram::label)
            .collect();
        assert_eq!(sys, ["system:deploy", "system:pair", "system:hand_limit"]);
        let src: Vec<&str> = ScriptSource::ALL
            .into_iter()
            .map(ScriptSource::as_str)
            .collect();
        assert_eq!(src, ["binding", "compiled", "vanilla"]);
    }

    #[test]
    fn step_params_name_their_hook() {
        let v = VarId(0);
        let sel = SelId(NodeId(0));
        let all = [
            StepParams::ReturnLookedBottom,
            StepParams::DeployExBase,
            StepParams::SupportUsed,
            StepParams::EB01aLookOwnTop {
                player: P::Active,
                var: v,
            },
            StepParams::EB01bLookTopOwn {
                player: P::Opp,
                var: v,
            },
            StepParams::GD02bPayDeployCost { var: v },
            StepParams::GD03aPayCostOf { var: v },
            StepParams::GD03aApMinusCount { var: v, sel },
            StepParams::GD04aCopyKeywords { var: v },
            StepParams::GD04aApDownByNamedTrashUnits {
                name: StrId(0),
                var: v,
            },
            StepParams::GD05aUseOnce { tag: -205_006 },
            StepParams::GD05bDestroyFirstShieldAreaCards { count: 2 },
            StepParams::GD05bResolvingCommandToTrash,
        ];
        let hooks: Vec<StepHook> = all.iter().map(StepParams::hook).collect();
        assert_eq!(hooks, StepHook::ALL);
    }

    #[test]
    fn kw_amounts_index_by_declaration_order() {
        let mut k = KwAmounts::default();
        k.0[Kw::Breach as usize] = Some(2);
        assert_eq!(k.get(Kw::Breach), Some(2));
        assert_eq!(k.get(Kw::Repair), None);
    }
}
