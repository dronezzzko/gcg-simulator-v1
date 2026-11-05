//! Whether a pending card choice helps or hurts the chosen cards (`ai/targeting.py`;
//! docs/port/inventory/07-ai-search.md §15).
//!
//! The resolving effect's program is public card text, so the policy reads the instructions
//! that follow the waiting `Choose` and classifies what happens to the chosen variable.
//!
//! Python memoises `choice_polarity` per `(program_id, pc, rule_table)`,
//! `choice_rule_lasts_this_battle` per `(program_id, pc)` and `command_harms_enemy_units` per
//! `def_id` in process globals (`ai/targeting.py:54-55`, `:93`). The memo tables are pure
//! functions of the registry, so these ports recompute them (open item O10 of
//! docs/port/contracts.md): the results are the same by construction.

use gcg_data::DefId;
use gcg_effects::{
    ContinuousId, Duration, Node, NodeId, RefId, Registry, RuleKind, RuleModId, Side, ValueRef,
    VarId,
};
use gcg_engine::GameState;

/// `HARMS` / `UNKNOWN` / `HELPS` (`ai/targeting.py:15-17`), with Python's integer values.
#[repr(i8)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Polarity {
    Harms = -1,
    Unknown = 0,
    Helps = 1,
}

impl Polarity {
    /// The Python integer (`-1`, `0`, `1`).
    #[must_use]
    pub const fn value(self) -> i32 {
        self as i32
    }
}

/// `RULE_POLARITY[kind]` (`ai/targeting.py:22-52`); the table covers every `RuleKind`
/// (`tests/ai/test_policy_knobs.py:108`).
#[must_use]
pub const fn rule_polarity_of(kind: RuleKind) -> Polarity {
    match kind {
        RuleKind::CantAttack
        | RuleKind::CantAttackPlayer
        | RuleKind::CantAttackUnits
        | RuleKind::CantBlock
        | RuleKind::CantBeSetActive
        | RuleKind::StaysRestedInStartPhase
        | RuleKind::DeployedRested
        | RuleKind::CantBePaired
        | RuleKind::MustAttack
        | RuleKind::AttackTargetFixed => Polarity::Harms,
        RuleKind::PlayCostDelta | RuleKind::PlayLevelDelta | RuleKind::Custom => Polarity::Unknown,
        RuleKind::CantBeBlocked
        | RuleKind::MayAttackActive
        | RuleKind::AttackOnDeployTurn
        | RuleKind::CantReceiveDamage
        | RuleKind::ReduceDamage
        | RuleKind::CantBeDestroyed
        | RuleKind::CantBeChosen
        | RuleKind::CantBeReturned
        | RuleKind::NoBlockerVs
        | RuleKind::CantBeAttacked
        | RuleKind::ForceAttackTarget
        | RuleKind::ShieldAreaProtection
        | RuleKind::ShieldAreaReduction
        | RuleKind::ApCantBeReduced
        | RuleKind::RedirectBattleDamage
        | RuleKind::RestSubstitute => Polarity::Helps,
    }
}

/// `choice_polarity(st, rule_table=False)` (`ai/targeting.py:58-71`): HELPS / HARMS / UNKNOWN
/// for the cards chosen at the waiting instruction of the top frame; UNKNOWN when no frame
/// waits. With `rule_table` a granted rule is judged by [`rule_polarity_of`]; otherwise every
/// `cant_` rule counts as harmful and every other rule as helpful.
#[must_use]
pub fn choice_polarity(st: &GameState, rule_table: bool) -> Polarity {
    let Some(f) = st.frames().last().filter(|f| f.waiting) else {
        return Polarity::Unknown;
    };
    let reg = st.registry();
    polarity_at(
        reg,
        &reg.program(f.program).instrs,
        usize::from(f.pc),
        rule_table,
    )
}

/// `choice_rule_lasts_this_battle(st)` (`ai/targeting.py:74-90`): whether the waiting `Choose`
/// grants the chosen cards a rule only for the current battle (an `Apply` of a `RuleGrant`
/// with duration `THIS_BATTLE`).
#[must_use]
pub fn choice_rule_lasts_this_battle(st: &GameState) -> bool {
    let Some(f) = st.frames().last().filter(|f| f.waiting) else {
        return false;
    };
    let reg = st.registry();
    rule_lasts_this_battle_at(reg, &reg.program(f.program).instrs, usize::from(f.pc))
}

/// The cached value of `choice_rule_lasts_this_battle` for `(program, pc)`
/// (`ai/targeting.py:83-88`).
#[must_use]
pub fn rule_lasts_this_battle_at(reg: &Registry, instrs: &[NodeId], pc: usize) -> bool {
    let Some(step) = first_step(reg, instrs, pc) else {
        return false;
    };
    match reg.node(instrs[step]) {
        Node::Apply(a) => {
            matches!(reg.node(a.effect.node()), Node::RuleGrant(_))
                && a.duration == Duration::ThisBattle
        }
        _ => false,
    }
}

/// `command_harms_enemy_units(def_id)` (`ai/targeting.py:96-112`): whether the card's Command
/// effect chooses enemy Units and harms them (removal). Always judged without the rule table.
///
/// Every Command ability of the registry has a program (`tests/policy_registry.rs` checks it
/// on the embedded IR), so Python's `R.programs[-1]` for a missing one never occurs; a Command
/// without a program has no `Choose` and is not removal here.
#[must_use]
pub fn command_harms_enemy_units(reg: &Registry, def: DefId) -> bool {
    let Some(aid) = reg.card(def).command_aid else {
        return false;
    };
    let Some(program) = reg.ability(aid).program else {
        return false;
    };
    let instrs = &reg.program(program).instrs;
    instrs.iter().enumerate().any(|(pc, &ins)| {
        matches!(reg.node(ins), Node::Choose(c) if sel_side(reg, c.sel.node()) == Some(Side::Enemy))
            && polarity_at(reg, instrs, pc, false) == Polarity::Harms
    })
}

/// `_polarity_at(instrs, pc, rule_table=False)` (`ai/targeting.py:115-123`).
#[must_use]
pub fn polarity_at(reg: &Registry, instrs: &[NodeId], pc: usize, rule_table: bool) -> Polarity {
    let Some(step) = first_step(reg, instrs, pc) else {
        return Polarity::Unknown;
    };
    let node = reg.node(instrs[step]);
    if is_harmful(node) {
        return Polarity::Harms;
    }
    if is_helpful(node) {
        return Polarity::Helps;
    }
    if let Node::Apply(a) = node {
        return effect_polarity(reg, a.effect, rule_table);
    }
    Polarity::Unknown
}

/// `_first_step(instrs, pc)` (`ai/targeting.py:126-137`): the index of the first harmful,
/// helpful or `Apply` instruction after `pc` whose `ref` is a `Var` or `Union` that uses the
/// variable chosen at `pc`; `None` when `instrs[pc]` is not a `Choose` or no such step follows.
/// `pc` must be an index of `instrs` (a waiting frame's `pc` always is).
#[must_use]
pub fn first_step(reg: &Registry, instrs: &[NodeId], pc: usize) -> Option<usize> {
    let Node::Choose(choose) = reg.node(instrs[pc]) else {
        return None;
    };
    let var = choose.var;
    for (i, &later) in instrs.iter().enumerate().skip(pc + 1) {
        let node = reg.node(later);
        let Some(r) = node_ref(node) else { continue };
        if !matches!(reg.node(r.node()), Node::Var(_) | Node::Union(_)) || !uses(reg, r, var) {
            continue;
        }
        if is_harmful(node) || is_helpful(node) || matches!(node, Node::Apply(_)) {
            return Some(i);
        }
    }
    None
}

/// `_uses(ref, var)` (`ai/targeting.py:140-145`).
fn uses(reg: &Registry, r: RefId, var: VarId) -> bool {
    match reg.node(r.node()) {
        Node::Var(v) => v.name == var,
        Node::Union(u) => u.refs.iter().any(|&x| uses(reg, x, var)),
        _ => false,
    }
}

/// `_effect_polarity(eff, rule_table=False)` (`ai/targeting.py:148-160`).
fn effect_polarity(reg: &Registry, eff: ContinuousId, rule_table: bool) -> Polarity {
    match reg.node(eff.node()) {
        Node::StatMod(m) => {
            let total: i64 = [m.ap, m.hp]
                .into_iter()
                .filter_map(|v| match v {
                    ValueRef::Lit(n) => Some(n),
                    ValueRef::Node(_) => None,
                })
                .sum();
            match total.signum() {
                0 => Polarity::Unknown,
                1 => Polarity::Helps,
                _ => Polarity::Harms,
            }
        }
        Node::KeywordGrant(_) | Node::TraitGrant(_) | Node::AbilityGrant(_) => Polarity::Helps,
        Node::RuleGrant(g) => {
            if rule_table {
                return rule_polarity(reg, g.rule);
            }
            let Node::RuleMod(rule) = reg.node(g.rule.node()) else {
                return Polarity::Unknown;
            };
            if rule.kind.as_str().starts_with("cant_") {
                Polarity::Harms
            } else {
                Polarity::Helps
            }
        }
        _ => Polarity::Unknown,
    }
}

/// `_rule_polarity(rule)` (`ai/targeting.py:163-166`): a non-zero literal cost or level delta
/// helps when negative and harms when positive; every other rule reads the table.
fn rule_polarity(reg: &Registry, rule: RuleModId) -> Polarity {
    let Node::RuleMod(m) = reg.node(rule.node()) else {
        return Polarity::Unknown;
    };
    if matches!(m.kind, RuleKind::PlayCostDelta | RuleKind::PlayLevelDelta)
        && let ValueRef::Lit(n) = m.amount
        && n != 0
    {
        return if n < 0 {
            Polarity::Helps
        } else {
            Polarity::Harms
        };
    }
    rule_polarity_of(m.kind)
}

/// `Choose.sel.side`.
fn sel_side(reg: &Registry, sel: NodeId) -> Option<Side> {
    match reg.node(sel) {
        Node::Sel(s) => Some(s.side),
        _ => None,
    }
}

/// `isinstance(step, _HARMFUL_STEPS)` (`ai/targeting.py:19`).
const fn is_harmful(node: &Node) -> bool {
    matches!(
        node,
        Node::Damage(_)
            | Node::Destroy(_)
            | Node::Rest(_)
            | Node::ReturnToHand(_)
            | Node::ToDeck(_)
            | Node::Exile(_)
            | Node::ToTrash(_)
    )
}

/// `isinstance(step, _HELPFUL_STEPS)` (`ai/targeting.py:20`).
const fn is_helpful(node: &Node) -> bool {
    matches!(
        node,
        Node::Recover(_)
            | Node::SetActive(_)
            | Node::AddToHand(_)
            | Node::DeployCard(_)
            | Node::PlayCard(_)
    )
}

/// `getattr(node, "ref", None)` (`ai/targeting.py:132`): the `ref` field of the 30 DSL
/// classes that have one (`None` when absent or `None`). `tests/policy_registry.rs` checks
/// this list against the schema table `gcg_effects::CLASSES`.
#[must_use]
pub fn node_ref(node: &Node) -> Option<RefId> {
    match node {
        Node::StatOf(n) => Some(n.r#ref),
        Node::KwAmount(n) => Some(n.r#ref),
        Node::NotRef(n) => Some(n.r#ref),
        Node::IsRef(n) => Some(n.r#ref),
        Node::LevelCmpRef(n) => Some(n.r#ref),
        Node::PairedPilotOf(n) => Some(n.r#ref),
        Node::PairedUnitOf(n) => Some(n.r#ref),
        Node::BattlingWith(n) => Some(n.r#ref),
        Node::RefMatches(n) => Some(n.r#ref),
        Node::RefEmpty(n) => Some(n.r#ref),
        Node::Damage(n) => Some(n.r#ref),
        Node::Destroy(n) => Some(n.r#ref),
        Node::Rest(n) => Some(n.r#ref),
        Node::SetActive(n) => Some(n.r#ref),
        Node::ReturnToHand(n) => Some(n.r#ref),
        Node::ToDeck(n) => Some(n.r#ref),
        Node::Exile(n) => Some(n.r#ref),
        Node::ToTrash(n) => Some(n.r#ref),
        Node::AddToHand(n) => Some(n.r#ref),
        Node::DeployCard(n) => Some(n.r#ref),
        Node::AddToShields(n) => n.r#ref,
        Node::Arrange(n) => Some(n.r#ref),
        Node::Reveal(n) => Some(n.r#ref),
        Node::Recover(n) => Some(n.r#ref),
        Node::Apply(n) => Some(n.r#ref),
        Node::PlayCard(n) => Some(n.r#ref),
        Node::ActivateMain(n) => Some(n.r#ref),
        Node::ForEach(n) => Some(n.r#ref),
        Node::BindVar(n) => Some(n.r#ref),
        Node::LoopInit(n) => n.r#ref,
        _ => None,
    }
}

/// The node kinds [`node_ref`] reads, for the schema check.
pub const REF_KINDS: [&str; 30] = [
    "StatOf",
    "KwAmount",
    "NotRef",
    "IsRef",
    "LevelCmpRef",
    "PairedPilotOf",
    "PairedUnitOf",
    "BattlingWith",
    "RefMatches",
    "RefEmpty",
    "Damage",
    "Destroy",
    "Rest",
    "SetActive",
    "ReturnToHand",
    "ToDeck",
    "Exile",
    "ToTrash",
    "AddToHand",
    "DeployCard",
    "AddToShields",
    "Arrange",
    "Reveal",
    "Recover",
    "Apply",
    "PlayCard",
    "ActivateMain",
    "ForEach",
    "BindVar",
    "LoopInit",
];
