//! The exhaustive target polarity of `exhaustive_polarity` (W-30): whether the cards chosen at
//! a waiting `Choose` are helped or harmed, from every step that acts on the chosen variable.
//!
//! The reference (`gcg_ai::targeting`, `ai/targeting.py:58-166`) reads only the first harmful,
//! helpful or `Apply` step on the variable and leaves CUSTOM rules, unsigned cost or level
//! deltas, cost modifiers and `StatMod`s with zero or computed totals UNKNOWN. Here:
//!
//! - every rule kind is signed by an exhaustive `match` ([`rule_kind_polarity`]), so a new
//!   kind fails to compile until it is signed; CUSTOM rules are signed per custom id by
//!   [`CUSTOM_RULES`] (the packaged registry has none; `tests.rs` fails on an unsigned one);
//! - PLAY_COST_DELTA and PLAY_LEVEL_DELTA, and `CostMod`, are signed by their amount's sign;
//! - a `StatMod` total is signed from the sign of each part, a computed part signed from its
//!   value node (counts, stats and sizes are never negative; `Times` multiplies signs);
//! - all steps on the variable are combined: one known polarity, or UNKNOWN when they
//!   disagree. An UNKNOWN choice is resolved by the look-ahead (`targets.rs`).

pub(crate) use gcg_ai::targeting::Polarity;
use gcg_ai::targeting::node_ref;
use gcg_effects::{ContinuousId, Node, NodeId, RefId, Registry, RuleKind, RuleModId, ValueRef};
use gcg_engine::GameState;

/// The polarity of each CUSTOM rule id (`RuleMod.name`). The packaged registry grants no
/// CUSTOM rule, so the table is empty; `tests.rs` lists every CUSTOM id of the registry and
/// fails on one missing here.
pub(crate) const CUSTOM_RULES: &[(&str, Polarity)] = &[];

/// The sign of a value: `Some(-1 | 0 | 1)`, or `None` when the IR does not fix it.
fn value_sign(reg: &Registry, v: ValueRef, depth: u32) -> Option<i64> {
    match v {
        ValueRef::Lit(n) => Some(n.signum()),
        ValueRef::Node(n) => node_sign(reg, n, depth),
    }
}

fn node_sign(reg: &Registry, n: NodeId, depth: u32) -> Option<i64> {
    if depth > 8 {
        return None;
    }
    match reg.node(n) {
        // Counts, stats, levels and sizes are never negative; zero only when nothing counts.
        Node::Count(_)
        | Node::StatOf(_)
        | Node::PlayerLevel(_)
        | Node::HandSize(_)
        | Node::ShieldCount(_)
        | Node::DeckSize(_)
        | Node::VarSize(_)
        | Node::EventAmount(_)
        | Node::KwAmount(_) => Some(1),
        Node::Times(t) => value_sign(reg, t.value, depth + 1).map(|s| s * t.factor.signum()),
        Node::Sum(s) => uniform_sign(reg, &s.terms, depth),
        Node::MinOf(m) => uniform_sign(reg, &m.terms, depth),
        _ => None,
    }
}

/// The common sign of every term, ignoring zeros; `None` when they disagree.
fn uniform_sign(reg: &Registry, terms: &[ValueRef], depth: u32) -> Option<i64> {
    let mut out = 0;
    for &t in terms {
        let s = value_sign(reg, t, depth + 1)?;
        if s != 0 {
            if out != 0 && out != s {
                return None;
            }
            out = s;
        }
    }
    Some(out)
}

const fn by_sign(sign: i64, positive: Polarity, negative: Polarity) -> Polarity {
    match sign {
        1.. => positive,
        0 => Polarity::Unknown,
        _ => negative,
    }
}

/// Every rule kind's polarity on the cards it is granted to; `None` for the kinds signed by
/// their amount (PLAY_COST_DELTA, PLAY_LEVEL_DELTA) or their custom id (CUSTOM).
pub(crate) const fn rule_kind_polarity(kind: RuleKind) -> Option<Polarity> {
    Some(match kind {
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
        RuleKind::PlayCostDelta | RuleKind::PlayLevelDelta | RuleKind::Custom => return None,
    })
}

/// The polarity of a granted rule.
pub(crate) fn rule_polarity(reg: &Registry, rule: RuleModId) -> Polarity {
    let Node::RuleMod(m) = reg.node(rule.node()) else {
        return Polarity::Unknown;
    };
    if let Some(p) = rule_kind_polarity(m.kind) {
        return p;
    }
    match m.kind {
        // A lower cost or level helps the card's controller.
        RuleKind::PlayCostDelta | RuleKind::PlayLevelDelta => value_sign(reg, m.amount, 0)
            .map_or(Polarity::Unknown, |s| {
                by_sign(s, Polarity::Harms, Polarity::Helps)
            }),
        _ => custom_rule_polarity(reg.str(m.name)),
    }
}

/// [`CUSTOM_RULES`]' entry for `name`, or UNKNOWN.
pub(crate) fn custom_rule_polarity(name: &str) -> Polarity {
    CUSTOM_RULES
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(Polarity::Unknown, |&(_, p)| p)
}

/// The polarity of a continuous effect on the cards it applies to.
pub(crate) fn effect_polarity(reg: &Registry, eff: ContinuousId) -> Polarity {
    match reg.node(eff.node()) {
        Node::StatMod(m) => {
            let (Some(a), Some(h)) = (value_sign(reg, m.ap, 0), value_sign(reg, m.hp, 0)) else {
                return Polarity::Unknown;
            };
            if a * h < 0 {
                return Polarity::Unknown;
            }
            by_sign(a + h, Polarity::Helps, Polarity::Harms)
        }
        Node::KeywordGrant(_) | Node::TraitGrant(_) | Node::AbilityGrant(_) => Polarity::Helps,
        Node::RuleGrant(g) => rule_polarity(reg, g.rule),
        Node::CostMod(c) => {
            let (Some(a), Some(b)) = (value_sign(reg, c.cost, 0), value_sign(reg, c.level, 0))
            else {
                return Polarity::Unknown;
            };
            if a * b < 0 {
                return Polarity::Unknown;
            }
            by_sign(a + b, Polarity::Harms, Polarity::Helps)
        }
        _ => Polarity::Unknown,
    }
}

/// The polarity of one step on the chosen cards, or `None` for a step that neither harms,
/// helps nor applies an effect.
fn step_polarity(reg: &Registry, node: &Node) -> Option<Polarity> {
    match node {
        Node::Damage(_)
        | Node::Destroy(_)
        | Node::Rest(_)
        | Node::ReturnToHand(_)
        | Node::ToDeck(_)
        | Node::Exile(_)
        | Node::ToTrash(_) => Some(Polarity::Harms),
        Node::Recover(_)
        | Node::SetActive(_)
        | Node::AddToHand(_)
        | Node::DeployCard(_)
        | Node::PlayCard(_) => Some(Polarity::Helps),
        Node::Apply(a) => Some(effect_polarity(reg, a.effect)),
        _ => None,
    }
}

/// Whether `r` is a `Var` or `Union` naming the variable `var`.
fn uses(reg: &Registry, r: RefId, var: gcg_effects::VarId) -> bool {
    match reg.node(r.node()) {
        Node::Var(v) => v.name == var,
        Node::Union(u) => u.refs.iter().any(|&x| uses(reg, x, var)),
        _ => false,
    }
}

/// The combined polarity of every step after `pc` on the variable `instrs[pc]` chooses;
/// UNKNOWN when `instrs[pc]` is not a `Choose`, no step acts on it, or the steps disagree.
pub(crate) fn polarity_at(reg: &Registry, instrs: &[NodeId], pc: usize) -> Polarity {
    let Some(Node::Choose(choose)) = instrs.get(pc).map(|&n| reg.node(n)) else {
        return Polarity::Unknown;
    };
    let mut out: Option<Polarity> = None;
    for &later in &instrs[pc + 1..] {
        let node = reg.node(later);
        if node_ref(node).is_none_or(|r| !uses(reg, r, choose.var)) {
            continue;
        }
        match step_polarity(reg, node) {
            None | Some(Polarity::Unknown) => {}
            Some(p) => match out {
                None => out = Some(p),
                Some(q) if q == p => {}
                Some(_) => return Polarity::Unknown,
            },
        }
    }
    out.unwrap_or(Polarity::Unknown)
}

/// The polarity of the cards chosen at the top frame's waiting `Choose` (`choice_polarity`
/// with the exhaustive table); UNKNOWN when no frame waits.
pub(crate) fn choice_polarity(st: &GameState) -> Polarity {
    let Some(f) = st.frames().last().filter(|f| f.waiting) else {
        return Polarity::Unknown;
    };
    let reg = st.registry();
    polarity_at(reg, &reg.program(f.program).instrs, usize::from(f.pc))
}

/// The amount of the first `Damage` step on the variable chosen at the top frame's waiting
/// `Choose`, when it is a literal (`kill_aware_targeting`, W-26).
pub(crate) fn choice_damage(st: &GameState) -> Option<i64> {
    let f = st.frames().last().filter(|f| f.waiting)?;
    let reg = st.registry();
    let instrs = &reg.program(f.program).instrs;
    let pc = usize::from(f.pc);
    let Node::Choose(choose) = reg.node(*instrs.get(pc)?) else {
        return None;
    };
    instrs[pc + 1..]
        .iter()
        .find_map(|&later| match reg.node(later) {
            Node::Damage(d) if uses(reg, d.r#ref, choose.var) => match d.amount {
                ValueRef::Lit(n) if n > 0 => Some(n),
                _ => None,
            },
            _ => None,
        })
}
