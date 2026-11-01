//! The frozen `Registry` API of docs/port/contracts.md (gcg-effects), over the contract's
//! `pub(crate)` layout. Every method reads through [`Registry::view`], the one place that
//! depends on that layout; the logic is [`crate::view::View`]'s, shared with the decoded
//! [`crate::ir::Tables`] that the crate's tests exercise.

use std::sync::atomic::{AtomicU32, Ordering};

use gcg_data::{CardDb, DefId, ImplementationStatus};

use crate::contracts::{
    AbilityEntry, AbilityId, CardEntry, CondId, ContRef, ContView, ContentId, ContinuousId, EvSet,
    FilterId, FilterSet, FilterSetId, HookParams, IrError, KeywordProgram, NodeId, Program,
    ProgramId, Registry, RuleModId, StrId, SystemProgram, VarId,
};
use crate::dsl::Node;
use crate::ir::Tables;
use crate::view::View;

/// Every registry gets a generation no other registry of the process has, so AI memo tables
/// keyed by it (W-33) never mix two registries. The value is only ever compared.
static NEXT_GENERATION: AtomicU32 = AtomicU32::new(0);

impl Tables {
    /// The registry of these tables over the card database `db`, with a fresh generation.
    pub(crate) fn into_registry(self, db: &'static CardDb) -> Registry {
        Registry {
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
            db,
            strings: self.strings,
            var_names: self.var_names,
            printable_non_ascii: self.printable_non_ascii,
            nodes: self.nodes,
            node_content_ids: self.node_content_ids,
            programs: self.programs,
            keyword_programs: self.keyword_programs,
            system_programs: self.system_programs,
            abilities: self.abilities,
            cards: self.cards,
            continuous: self.continuous,
            filter_sets: self.filter_sets,
            rule_mod_reprs: self.rule_mod_reprs,
            aliases: self.aliases,
            always_events: self.always_events,
            hp_reduction_possible: self.hp_reduction_possible,
            token_keys: self.token_keys,
            unit_variants: self.unit_variants,
            hook_params: self.hook_params,
        }
    }

    /// The tables of a registry (without the golden-only card texts, which the `Registry`
    /// layout does not hold).
    fn from_registry(reg: &Registry) -> Self {
        Self {
            strings: reg.strings.clone(),
            var_names: reg.var_names.clone(),
            printable_non_ascii: reg.printable_non_ascii.clone(),
            nodes: reg.nodes.clone(),
            node_content_ids: reg.node_content_ids.clone(),
            programs: reg.programs.clone(),
            keyword_programs: reg.keyword_programs,
            system_programs: reg.system_programs,
            abilities: reg.abilities.clone(),
            cards: reg.cards.clone(),
            continuous: reg.continuous.clone(),
            filter_sets: reg.filter_sets.clone(),
            rule_mod_reprs: reg.rule_mod_reprs.clone(),
            aliases: reg.aliases.clone(),
            always_events: reg.always_events,
            hp_reduction_possible: reg.hp_reduction_possible,
            token_keys: reg.token_keys.clone(),
            unit_variants: reg.unit_variants.clone(),
            hook_params: reg.hook_params.clone(),
            card_texts: Box::new([]),
        }
    }
}

impl Registry {
    /// Loads an effect IR (PLAN §3.3-§3.4) as the registry of the card database `db`: the IR
    /// version, its schema, enums and hooks against this build's DSL types, every node and
    /// handle kind, and the cross-table invariants are checked; the first violation is the
    /// error (an unknown class or hook names its card, W-69).
    ///
    /// # Errors
    /// [`IrError`] for an IR this build cannot load.
    pub fn from_ir(ir_json: &str, db: &'static CardDb) -> Result<Self, IrError> {
        Ok(Tables::decode(ir_json)?.into_registry(db))
    }

    /// The registry with a script-override fragment applied (`registry.py:209-220`
    /// `override_script`; PLAN §3.4 "Test overrides"; the format of
    /// `port/oracle/export_ir.py` `build_fragment`). The fragment's values are applied
    /// verbatim; the result has a new generation (W-33).
    ///
    /// # Errors
    /// [`IrError::DataMismatch`] when the fragment was exported against another base
    /// registry, otherwise [`IrError`] for a fragment this build cannot load.
    pub fn with_fragment(&self, fragment: &serde_json::Value) -> Result<Self, IrError> {
        Ok(Tables::from_registry(self)
            .with_fragment(fragment)?
            .into_registry(self.db))
    }

    fn view(&self) -> View<'_> {
        View {
            strings: &self.strings,
            var_names: &self.var_names,
            nodes: &self.nodes,
            node_content_ids: &self.node_content_ids,
            programs: &self.programs,
            keyword_programs: &self.keyword_programs,
            system_programs: &self.system_programs,
            abilities: &self.abilities,
            cards: &self.cards,
            continuous: &self.continuous,
            filter_sets: &self.filter_sets,
            rule_mod_reprs: &self.rule_mod_reprs,
            aliases: &self.aliases,
            hook_params: &self.hook_params,
        }
    }

    /// `Registry.db` (`effects/registry.py:70`).
    #[must_use]
    pub fn db(&self) -> &'static CardDb {
        self.db
    }

    /// Changes on every `with_fragment`; AI memo tables key on it (W-33).
    #[must_use]
    pub fn generation(&self) -> u32 {
        self.generation
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> &Node {
        self.view().node(id)
    }

    /// `Registry.programs[id]` (`effects/registry.py:71`).
    #[must_use]
    pub fn program(&self, id: ProgramId) -> &Program {
        self.view().program(id)
    }

    /// `Registry.abilities[aid]` (`effects/registry.py:72`).
    #[must_use]
    pub fn ability(&self, aid: AbilityId) -> &AbilityEntry {
        self.view().ability(aid)
    }

    /// `Registry.card(def_id)` (`effects/registry.py:140-141`).
    #[must_use]
    pub fn card(&self, def: DefId) -> &CardEntry {
        self.view().card(def)
    }

    #[must_use]
    pub fn str(&self, id: StrId) -> &str {
        self.view().str(id)
    }

    #[must_use]
    pub fn var_name(&self, v: VarId) -> &str {
        self.view().var_name(v)
    }

    /// `repair_program`, `breach_program`, `support_program` (`registry.py:327-329`).
    #[must_use]
    pub fn keyword_program(&self, kw: KeywordProgram) -> ProgramId {
        self.view().keyword_program(kw)
    }

    /// `interp.system_program(name)` (`engine/interp.py:1581-1594`), at the IR's fixed ids.
    #[must_use]
    pub fn system_program(&self, s: SystemProgram) -> ProgramId {
        self.view().system_program(s)
    }

    /// `reg.aliases.get(def_id, ())` (`registry.py:240-241`).
    #[must_use]
    pub fn aliases(&self, def: DefId) -> &[StrId] {
        self.view().aliases(def)
    }

    /// `reg.always_events` (`registry.py:356-378`).
    #[must_use]
    pub fn always_events(&self) -> EvSet {
        self.always_events
    }

    /// `reg.events_by_def[def_id]` (`registry.py:356-397`).
    #[must_use]
    pub fn events_of(&self, def: DefId) -> EvSet {
        self.view().events_of(def)
    }

    /// `reg.hp_reduction_possible` (`registry.py:356-378`).
    #[must_use]
    pub fn hp_reduction_possible(&self) -> bool {
        self.hp_reduction_possible
    }

    /// The typed parameters of a `Custom*` node (PLAN §5.3).
    #[must_use]
    pub fn hook_params(&self, node: NodeId) -> Option<&HookParams> {
        self.view().hook_params(node)
    }

    /// The canonical `ContRef` of an IR `Continuous` node (PLAN §5.6, `registry.py:124-130`).
    #[must_use]
    pub fn cont_of(&self, n: ContinuousId) -> ContRef {
        self.view().cont_of(n)
    }

    /// Typed read access for `view._apply_cont` (`engine/view.py:347-406`).
    #[must_use]
    pub fn cont_node(&self, c: ContRef) -> ContView<'_> {
        self.view().cont_node(c)
    }

    /// `reg.filter_sets[k]` (`registry.py:132-138`).
    #[must_use]
    pub fn filter_set(&self, f: FilterSetId) -> &FilterSet {
        self.view().filter_set(f)
    }

    /// The program with this content id (PLAN §4.8; the exporter asserts they are unique).
    #[must_use]
    pub fn program_by_content_id(&self, cid: &ContentId) -> Option<ProgramId> {
        self.view().program_by_content_id(cid)
    }

    /// The IR continuous effect with this content id (PLAN §4.8).
    #[must_use]
    pub fn cont_by_content_id(&self, cid: &ContentId) -> Option<ContRef> {
        self.view().cont_by_content_id(cid)
    }

    /// The IR filter set with this content id (PLAN §4.8).
    #[must_use]
    pub fn filter_set_by_content_id(&self, cid: &ContentId) -> Option<FilterSetId> {
        self.view().filter_set_by_content_id(cid)
    }

    /// `content_id(node)` (`runner/digest.py:26-27`).
    #[must_use]
    pub fn node_content_id(&self, id: NodeId) -> ContentId {
        self.view().node_content_id(id)
    }

    /// `content_id(reg.filter_sets[k])` (`runner/digest.py:40-48`).
    #[must_use]
    pub fn filter_set_content_id(&self, f: FilterSetId) -> ContentId {
        self.view().filter_set_content_id(f)
    }

    /// `repr(rule)` (`ai/actions.py:360` sort key).
    #[must_use]
    pub fn rule_mod_repr(&self, rule: RuleModId) -> &str {
        self.view().rule_mod_repr(rule)
    }

    /// CPython `repr` of a node (PLAN §5.7).
    #[must_use]
    pub fn repr(&self, id: NodeId) -> String {
        self.view().repr(id)
    }

    /// The `VarId` of a frame-variable name (IR `var_names`).
    #[must_use]
    pub fn var_id(&self, name: &str) -> Option<VarId> {
        self.view().var_id(name)
    }

    /// The `StrId` of a string of the IR string table.
    #[must_use]
    pub fn str_id(&self, s: &str) -> Option<StrId> {
        self.view().str_id(s)
    }

    /// `filters_key(filters)` (`registry.py:132-138`) for an IR filter tuple.
    #[must_use]
    pub fn filters_key(&self, filters: &[FilterId]) -> Option<FilterSetId> {
        self.view().filters_key(filters)
    }

    /// `n` as a `Filter` handle, if its node is one.
    #[must_use]
    pub fn filter_id(&self, n: NodeId) -> Option<FilterId> {
        self.view().filter_id(n)
    }

    /// `n` as a `Cond` handle, if its node is one.
    #[must_use]
    pub fn cond_id(&self, n: NodeId) -> Option<CondId> {
        self.view().cond_id(n)
    }

    /// `n` as a `Continuous` handle, if its node is one.
    #[must_use]
    pub fn continuous_id(&self, n: NodeId) -> Option<ContinuousId> {
        self.view().continuous_id(n)
    }
}

impl ContRef {
    /// `repr` of the effect (PLAN §4.7): `StatMod(ap=.., hp=..)`, `KeywordGrant(..)`,
    /// `AbilityGrant(..)` for value variants, the node's `repr` otherwise.
    #[must_use]
    pub fn repr(&self, reg: &Registry) -> String {
        reg.view().cont_repr(*self)
    }

    /// `content_id(effect)` (`runner/digest.py:26-27`).
    #[must_use]
    pub fn content_id(&self, reg: &Registry) -> ContentId {
        reg.view().cont_content_id(*self)
    }
}

/// `registry.cards[def_id].script is None` (`deck/validate.py:255-256`). A loaded IR never
/// holds a card without a script (W-69: the loader refuses one), so this is always `None`.
impl ImplementationStatus for Registry {
    fn unimplemented_error(&self, def: DefId) -> Option<&str> {
        self.card(def)
            .script
            .is_none()
            .then_some("no script in the effect IR")
    }
}
