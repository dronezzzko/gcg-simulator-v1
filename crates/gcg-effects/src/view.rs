//! Read access to the registry tables, shared by [`crate::Registry`] and the decoded
//! [`crate::ir::Tables`]: one implementation of every lookup, of continuous-effect identity
//! (PLAN §5.6), of CPython `repr` and of content ids (PLAN §5.7).

use gcg_data::DefId;

use crate::codec::ReprCx;
use crate::contracts::{
    AbilityEntry, CardEntry, CondId, ContRef, ContView, ContentId, ContinuousId, EvSet, FilterId,
    FilterSet, FilterSetId, HookParams, KeywordProgram, NodeId, Program, ProgramId, RuleModId,
    StrId, SystemProgram, ValueRef, VarId,
};
use crate::dsl::{self, Node, NodeKind};
use crate::pyrepr::content_id_of;

/// Borrowed registry tables (the `Registry` layout of `crate::contracts`).
#[derive(Clone, Copy)]
pub(crate) struct View<'a> {
    pub strings: &'a [Box<str>],
    pub var_names: &'a [Box<str>],
    pub nodes: &'a [Node],
    pub node_content_ids: &'a [ContentId],
    pub programs: &'a [Program],
    pub keyword_programs: &'a [ProgramId; 3],
    pub system_programs: &'a [ProgramId; 3],
    pub abilities: &'a [AbilityEntry],
    pub cards: &'a [CardEntry],
    pub continuous: &'a [(ContinuousId, ContRef)],
    pub filter_sets: &'a [FilterSet],
    pub rule_mod_reprs: &'a [(RuleModId, Box<str>)],
    pub aliases: &'a [(DefId, Box<[StrId]>)],
    pub hook_params: &'a [(NodeId, HookParams)],
}

/// The index of `s` in a table that is sorted, or sorted in two segments after a fragment
/// (PLAN §3.4): a binary search, then a scan when the search misses.
fn find_str(table: &[Box<str>], s: &str) -> Option<usize> {
    match table.binary_search_by(|x| (**x).cmp(s)) {
        Ok(i) => Some(i),
        Err(_) => table.iter().position(|x| &**x == s),
    }
}

/// The canonical [`ContRef`] of a `Continuous` node (PLAN §5.6): value-shaped effects (int
/// amounts) are value variants whatever their origin; every other continuous node is
/// `ContRef::Node`. `None` for a node that is not `Continuous`, or whose int amount does not
/// fit the variant (the exporter asserts none does).
pub(crate) fn canonical_cont(id: NodeId, node: &Node) -> Option<ContRef> {
    Some(match node {
        Node::StatMod(dsl::StatMod {
            ap: ValueRef::Lit(ap),
            hp: ValueRef::Lit(hp),
        }) => ContRef::StatMod {
            ap: i32::try_from(*ap).ok()?,
            hp: i32::try_from(*hp).ok()?,
        },
        Node::KeywordGrant(dsl::KeywordGrant {
            keyword,
            amount: ValueRef::Lit(n),
        }) => ContRef::KeywordGrant {
            kw: *keyword,
            amount: i32::try_from(*n).ok()?,
        },
        Node::AbilityGrant(g) => ContRef::AbilityGrant {
            card: g.card_number,
            index: i32::try_from(g.ability_index).ok()?,
        },
        Node::StatMod(_)
        | Node::KeywordGrant(_)
        | Node::TraitGrant(_)
        | Node::RuleGrant(_)
        | Node::CostMod(_) => ContRef::Node(id),
        _ => return None,
    })
}

impl<'a> View<'a> {
    pub(crate) fn repr_cx(&self) -> ReprCx<'a> {
        ReprCx {
            nodes: self.nodes,
            strings: self.strings,
            var_names: self.var_names,
        }
    }

    pub(crate) fn node(&self, id: NodeId) -> &'a Node {
        &self.nodes[id.0 as usize]
    }

    pub(crate) fn program(&self, id: ProgramId) -> &'a Program {
        &self.programs[usize::from(id.0)]
    }

    pub(crate) fn ability(&self, aid: crate::contracts::AbilityId) -> &'a AbilityEntry {
        &self.abilities[usize::from(aid.0)]
    }

    pub(crate) fn card(&self, def: DefId) -> &'a CardEntry {
        &self.cards[usize::from(def.0)]
    }

    pub(crate) fn str(&self, id: StrId) -> &'a str {
        &self.strings[id.0 as usize]
    }

    pub(crate) fn var_name(&self, v: VarId) -> &'a str {
        &self.var_names[usize::from(v.0)]
    }

    pub(crate) fn keyword_program(&self, kw: KeywordProgram) -> ProgramId {
        self.keyword_programs[kw as usize]
    }

    pub(crate) fn system_program(&self, s: SystemProgram) -> ProgramId {
        self.system_programs[s as usize]
    }

    /// `reg.aliases.get(def_id, ())` (`registry.py:240-241`).
    pub(crate) fn aliases(&self, def: DefId) -> &'a [StrId] {
        self.aliases
            .binary_search_by_key(&def, |(d, _)| *d)
            .map_or(&[], |i| &self.aliases[i].1)
    }

    pub(crate) fn events_of(&self, def: DefId) -> EvSet {
        self.card(def).events
    }

    pub(crate) fn hook_params(&self, node: NodeId) -> Option<&'a HookParams> {
        self.hook_params
            .binary_search_by_key(&node, |(n, _)| *n)
            .ok()
            .map(|i| &self.hook_params[i].1)
    }

    /// The canonical `ContRef` of an IR `Continuous` node, as the IR's `continuous` table
    /// lists it (and as [`canonical_cont`] computes it; the loader checks both agree).
    pub(crate) fn cont_of(&self, n: ContinuousId) -> ContRef {
        match self.continuous.binary_search_by_key(&n, |(c, _)| *c) {
            Ok(i) => self.continuous[i].1,
            Err(_) => {
                canonical_cont(n.node(), self.node(n.node())).unwrap_or(ContRef::Node(n.node()))
            }
        }
    }

    /// Typed read access for `view._apply_cont` (`engine/view.py:347-406`).
    pub(crate) fn cont_node(&self, c: ContRef) -> ContView<'a> {
        match c {
            ContRef::StatMod { ap, hp } => ContView::StatMod {
                ap: ValueRef::Lit(i64::from(ap)),
                hp: ValueRef::Lit(i64::from(hp)),
            },
            ContRef::KeywordGrant { kw, amount } => ContView::KeywordGrant {
                keyword: kw,
                amount: ValueRef::Lit(i64::from(amount)),
            },
            ContRef::AbilityGrant { card, index } => ContView::AbilityGrant {
                card_number: card,
                ability_index: i64::from(index),
            },
            ContRef::Node(id) => match self.node(id) {
                Node::StatMod(x) => ContView::StatMod { ap: x.ap, hp: x.hp },
                Node::KeywordGrant(x) => ContView::KeywordGrant {
                    keyword: x.keyword,
                    amount: x.amount,
                },
                Node::TraitGrant(x) => ContView::TraitGrant { traits: &x.traits },
                Node::RuleGrant(x) => ContView::RuleGrant { rule: x.rule },
                Node::AbilityGrant(x) => ContView::AbilityGrant {
                    card_number: x.card_number,
                    ability_index: x.ability_index,
                },
                Node::CostMod(x) => ContView::CostMod {
                    cost: x.cost,
                    level: x.level,
                },
                other => unreachable_cont(other),
            },
        }
    }

    pub(crate) fn filter_set(&self, f: FilterSetId) -> &'a FilterSet {
        &self.filter_sets[usize::from(f.0)]
    }

    /// `filters_key` (`registry.py:132-138`) for a filter tuple of the IR.
    pub(crate) fn filters_key(&self, filters: &[FilterId]) -> Option<FilterSetId> {
        self.filter_sets
            .iter()
            .position(|f| &*f.filters == filters)
            .and_then(|i| u16::try_from(i).ok())
            .map(FilterSetId)
    }

    pub(crate) fn program_by_content_id(&self, cid: &ContentId) -> Option<ProgramId> {
        self.programs
            .iter()
            .position(|p| p.content_id == *cid)
            .and_then(|i| u16::try_from(i).ok())
            .map(ProgramId)
    }

    /// The `ContRef` of the IR continuous node with this content id. Runtime values that no
    /// IR node spells (a frozen `StatMod`) are not found; snapshots carry them as values.
    pub(crate) fn cont_by_content_id(&self, cid: &ContentId) -> Option<ContRef> {
        self.continuous
            .iter()
            .find(|(n, _)| self.node_content_ids[n.node().0 as usize] == *cid)
            .map(|(_, c)| *c)
    }

    pub(crate) fn filter_set_by_content_id(&self, cid: &ContentId) -> Option<FilterSetId> {
        self.filter_sets
            .iter()
            .position(|f| f.content_id == *cid)
            .and_then(|i| u16::try_from(i).ok())
            .map(FilterSetId)
    }

    pub(crate) fn node_content_id(&self, id: NodeId) -> ContentId {
        self.node_content_ids[id.0 as usize]
    }

    pub(crate) fn filter_set_content_id(&self, f: FilterSetId) -> ContentId {
        self.filter_set(f).content_id
    }

    /// `repr(r.rule)` (`ai/actions.py:360` sort key), from the IR's `rule_mods` table.
    pub(crate) fn rule_mod_repr(&self, rule: RuleModId) -> &'a str {
        // The loader lists every RuleMod node (IR `rule_mods`, checked complete).
        let i = self
            .rule_mod_reprs
            .binary_search_by_key(&rule, |(r, _)| *r)
            .unwrap_or_else(|_| panic!("RuleMod node {} has no repr entry", rule.node().0));
        &self.rule_mod_reprs[i].1
    }

    /// CPython `repr` of a node.
    pub(crate) fn repr(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.node(id).repr_into(&self.repr_cx(), &mut out);
        out
    }

    /// `repr(Program(instrs, label))` (`effects/program.py:90-93`); the loader takes program
    /// content ids from the IR, and the content-id test recomputes them with this.
    #[cfg(test)]
    pub(crate) fn program_repr(&self, id: ProgramId) -> String {
        use crate::codec::Field as _;
        use crate::pyrepr::repr_str;
        let p = self.program(id);
        let cx = self.repr_cx();
        let mut out = String::from("Program(instrs=");
        p.instrs.repr(&cx, &mut out);
        out.push_str(", label=");
        out.push_str(&repr_str(self.str(p.label)));
        out.push(')');
        out
    }

    /// `repr` of a continuous effect value (PLAN §4.7): value variants print as the
    /// dataclass Python builds at run time.
    pub(crate) fn cont_repr(&self, c: ContRef) -> String {
        let node = match c {
            ContRef::Node(id) => return self.repr(id),
            ContRef::StatMod { ap, hp } => Node::StatMod(dsl::StatMod {
                ap: ValueRef::Lit(i64::from(ap)),
                hp: ValueRef::Lit(i64::from(hp)),
            }),
            ContRef::KeywordGrant { kw, amount } => Node::KeywordGrant(dsl::KeywordGrant {
                keyword: kw,
                amount: ValueRef::Lit(i64::from(amount)),
            }),
            ContRef::AbilityGrant { card, index } => Node::AbilityGrant(dsl::AbilityGrant {
                card_number: card,
                ability_index: i64::from(index),
            }),
        };
        let mut out = String::new();
        node.repr_into(&self.repr_cx(), &mut out);
        out
    }

    /// `content_id(effect)` (`runner/digest.py:26-27,41`).
    pub(crate) fn cont_content_id(&self, c: ContRef) -> ContentId {
        match c {
            ContRef::Node(id) => self.node_content_id(id),
            _ => content_id_of(&self.cont_repr(c)),
        }
    }

    pub(crate) fn var_id(&self, name: &str) -> Option<VarId> {
        find_str(self.var_names, name)
            .and_then(|i| u16::try_from(i).ok())
            .map(VarId)
    }

    pub(crate) fn str_id(&self, s: &str) -> Option<StrId> {
        find_str(self.strings, s)
            .and_then(|i| u32::try_from(i).ok())
            .map(StrId)
    }

    fn kind(&self, n: NodeId) -> Option<NodeKind> {
        self.nodes.get(n.0 as usize).map(Node::kind)
    }

    pub(crate) fn filter_id(&self, n: NodeId) -> Option<FilterId> {
        self.kind(n)
            .filter(|k| NodeKind::FILTER.contains(k))
            .map(|_| FilterId(n))
    }

    pub(crate) fn cond_id(&self, n: NodeId) -> Option<CondId> {
        self.kind(n)
            .filter(|k| NodeKind::COND.contains(k))
            .map(|_| CondId(n))
    }

    pub(crate) fn continuous_id(&self, n: NodeId) -> Option<ContinuousId> {
        self.kind(n)
            .filter(|k| NodeKind::CONTINUOUS.contains(k))
            .map(|_| ContinuousId(n))
    }
}

/// A `ContRef::Node` always names a `Continuous` node: the loader builds them only from
/// nodes of that alias, and `ContinuousId` is checked at decode.
fn unreachable_cont(node: &Node) -> ! {
    panic!(
        "ContRef::Node names a {} node, which is not Continuous",
        node.kind().py_name()
    )
}

impl View<'_> {
    /// The content fingerprint that names the base registry of an IR fragment
    /// (`export_ir.registry_fingerprint`): SHA-256 over every node content id, every program
    /// content id, and the ability and card counts.
    pub(crate) fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for cid in self.node_content_ids {
            h.update(cid.as_str().as_bytes());
            h.update(b"\n");
        }
        h.update(b"programs\n");
        for p in self.programs {
            h.update(p.content_id.as_str().as_bytes());
            h.update(b"\n");
        }
        h.update(
            format!(
                "abilities {}\ncards {}\n",
                self.abilities.len(),
                self.cards.len()
            )
            .as_bytes(),
        );
        h.finalize()
            .iter()
            .fold(String::with_capacity(64), |mut s, b| {
                use std::fmt::Write as _;
                let _ = write!(s, "{b:02x}");
                s
            })
    }
}
