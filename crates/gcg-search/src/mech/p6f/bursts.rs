//! What a program does to its controller (W-163, W-27, W-28): the review of a Burst or of a
//! "you may" body (a cost, a step that harms its controller's own cards, an optional
//! replacement, or a custom step whose effect the IR does not show), and whether a play's
//! effect outlasts the turn.
//!
//! The reference accepts every Burst with the presets' high score (`policy_burst_yes_score`,
//! `ai/policy.py:309-310`), which is right for every Burst of the packaged pool whose review
//! is clean. The W-163 tripwire (`tests.rs`) pins the list of packaged Bursts whose review is
//! not clean, so a new card on it fails until it is reviewed.

use std::ops::Range;

use gcg_effects::{Duration, Node, NodeId, P, ProgramId, RefId, Registry, Side};
use gcg_engine::{Action, ActionKind, BURST_PROMPT, GameState, SUPPORT_AID, Uid};

use super::polarity::{Polarity, effect_polarity};

/// The review of a program or a part of it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct Review {
    /// A cost step (`PayResources`, `RestCards`, `DiscardCards`, `TrashSelf`, ...).
    pub(crate) cost: bool,
    /// A step that discards, mills, damages, destroys, rests, returns, exiles or trashes its
    /// controller's own cards or damages its controller (adding cards to the hand and deploying
    /// the card itself are not harm).
    pub(crate) self_harm: bool,
    /// A nested "you may": an optional replacement or an optional extra.
    pub(crate) optional: bool,
    /// A custom step, whose effect the IR does not show.
    pub(crate) custom: bool,
}

impl Review {
    pub(crate) const fn is_clean(self) -> bool {
        !(self.cost || self.self_harm || self.optional || self.custom)
    }

    /// The reasons, for the tripwire's list.
    #[cfg(test)]
    pub(crate) fn reasons(self) -> Vec<&'static str> {
        [
            (self.cost, "cost"),
            (self.self_harm, "self_harm"),
            (self.optional, "optional"),
            (self.custom, "custom"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
    }
}

/// Whose cards a reference names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Owner {
    /// The card that carries the effect.
    ThisCard,
    Own,
    Enemy,
    /// Either player's, or not known from the IR.
    Unknown,
}

impl Owner {
    const fn includes_own(self) -> bool {
        !matches!(self, Self::Enemy)
    }
}

/// The review of the whole program `program` (and nothing else): every instruction except the
/// Burst prompt's own `AskMay`.
pub(crate) fn review_program(reg: &Registry, program: ProgramId) -> Review {
    let instrs = &reg.program(program).instrs;
    review(reg, instrs, 0..instrs.len())
}

/// The review of the body of the "you may" the top frame waits at, or `None` when the top
/// frame does not wait at an `AskMay`.
pub(crate) fn waiting_may_review(s: &GameState) -> Option<Review> {
    let f = s.frames().last().filter(|f| f.waiting)?;
    let reg = s.registry();
    let instrs = &reg.program(f.program).instrs;
    let pc = usize::from(f.pc);
    let Node::AskMay(ask) = reg.node(*instrs.get(pc)?) else {
        return None;
    };
    let end = usize::try_from(ask.target_if_no)
        .ok()
        .filter(|&e| e > pc && e <= instrs.len())
        .unwrap_or(instrs.len());
    Some(review(reg, instrs, pc + 1..end))
}

/// The review of `instrs[range]`.
pub(crate) fn review(reg: &Registry, instrs: &[NodeId], range: Range<usize>) -> Review {
    let mut r = Review::default();
    for i in range {
        let node = reg.node(instrs[i]);
        match node {
            Node::RestSelf(_)
            | Node::PayResources(_)
            | Node::RestCards(_)
            | Node::DestroyCards(_)
            | Node::DiscardCards(_)
            | Node::ExileCards(_)
            | Node::ReturnSelf(_)
            | Node::DestroySelf(_)
            | Node::TrashSelf(_)
            | Node::PayCost(_) => r.cost = true,
            Node::AskMay(a) => {
                if reg.str(a.prompt) != BURST_PROMPT {
                    r.optional = true;
                }
            }
            Node::CustomStep(_) => r.custom = true,
            Node::Discard(d) => r.self_harm |= d.player == P::You,
            Node::Mill(m) => r.self_harm |= m.player == P::You,
            Node::DamagePlayer(d) => r.self_harm |= d.player == P::You,
            Node::DamageShieldArea(d) => r.self_harm |= d.player == P::You,
            Node::RestResources(_) => r.self_harm = true,
            Node::Damage(n) => r.self_harm |= owner(reg, instrs, n.r#ref).includes_own(),
            Node::Destroy(n) => r.self_harm |= owner(reg, instrs, n.r#ref).includes_own(),
            Node::Rest(n) => r.self_harm |= owner(reg, instrs, n.r#ref).includes_own(),
            Node::ReturnToHand(n) => r.self_harm |= owner(reg, instrs, n.r#ref).includes_own(),
            Node::ToDeck(n) => r.self_harm |= owner(reg, instrs, n.r#ref).includes_own(),
            Node::Exile(n) => r.self_harm |= owner(reg, instrs, n.r#ref).includes_own(),
            Node::ToTrash(n) => r.self_harm |= owner(reg, instrs, n.r#ref).includes_own(),
            Node::Apply(a) => {
                r.self_harm |= effect_polarity(reg, a.effect) == Polarity::Harms
                    && owner(reg, instrs, a.r#ref).includes_own();
            }
            _ => {}
        }
    }
    r
}

/// Whose cards `r` names in a program with instructions `instrs`.
fn owner(reg: &Registry, instrs: &[NodeId], r: RefId) -> Owner {
    owner_of(reg, instrs, r.node(), 0)
}

fn owner_of(reg: &Registry, instrs: &[NodeId], node: NodeId, depth: u32) -> Owner {
    if depth > 8 {
        return Owner::Unknown;
    }
    match reg.node(node) {
        Node::This(_) | Node::ThisCard(_) => Owner::ThisCard,
        Node::All(a) => sel_owner(reg, a.sel.node()),
        Node::PairedPilotOf(p) => owner_of(reg, instrs, p.r#ref.node(), depth + 1),
        Node::PairedUnitOf(p) => owner_of(reg, instrs, p.r#ref.node(), depth + 1),
        Node::Union(u) => {
            let owners: Vec<Owner> = u
                .refs
                .iter()
                .map(|x| owner_of(reg, instrs, x.node(), depth + 1))
                .collect();
            if owners.is_empty() || owners.contains(&Owner::Unknown) {
                Owner::Unknown
            } else if owners.iter().all(|&o| o == owners[0]) {
                owners[0]
            } else if owners.contains(&Owner::Enemy) {
                Owner::Unknown
            } else {
                Owner::Own
            }
        }
        Node::Var(v) => {
            for &ins in instrs {
                match reg.node(ins) {
                    Node::Choose(c) if c.var == v.name => return sel_owner(reg, c.sel.node()),
                    Node::BindVar(b) if b.var == v.name => {
                        return owner_of(reg, instrs, b.r#ref.node(), depth + 1);
                    }
                    Node::ForEach(f) if f.var == v.name => {
                        return owner_of(reg, instrs, f.r#ref.node(), depth + 1);
                    }
                    Node::LookTop(l) if l.var == v.name => {
                        return if l.player == P::You {
                            Owner::Own
                        } else {
                            Owner::Unknown
                        };
                    }
                    Node::DeployToken(t) if t.var == v.name => {
                        return if t.player == P::You {
                            Owner::Own
                        } else {
                            Owner::Unknown
                        };
                    }
                    _ => {}
                }
            }
            Owner::Unknown
        }
        _ => Owner::Unknown,
    }
}

fn sel_owner(reg: &Registry, sel: NodeId) -> Owner {
    match reg.node(sel) {
        Node::Sel(s) => match s.side {
            Side::Friendly => Owner::Own,
            Side::Enemy => Owner::Enemy,
            Side::Any => Owner::Unknown,
        },
        _ => Owner::Unknown,
    }
}

/// The program a MAIN or ACTION_STEP play resolves: a Command's effect or an activated
/// ability's (not the <Support> keyword action).
pub(crate) fn play_program(s: &GameState, option: Action) -> Option<ProgramId> {
    let reg = s.registry();
    let uid = Uid::try_from(option.a)
        .ok()
        .filter(|&u| usize::from(u) < s.cards().len())?;
    match option.kind {
        ActionKind::PlayCommand => {
            let aid = reg.card(s.card(uid).def_id).command_aid?;
            reg.ability(aid).program
        }
        ActionKind::Activate if option.b != SUPPORT_AID => {
            let aid = u16::try_from(option.b).ok()?;
            reg.ability(gcg_effects::AbilityId(aid)).program
        }
        _ => None,
    }
}

/// W-28: whether a play's effect outlasts the turn: it rests, destroys, damages, returns,
/// exiles or trashes a card, or applies an effect beyond this turn and this battle.
pub(crate) fn play_outlasts_turn(s: &GameState, option: Action) -> bool {
    let Some(program) = play_program(s, option) else {
        return false;
    };
    let reg = s.registry();
    reg.program(program)
        .instrs
        .iter()
        .any(|&ins| match reg.node(ins) {
            Node::Rest(_)
            | Node::Destroy(_)
            | Node::Damage(_)
            | Node::DamageShieldArea(_)
            | Node::ReturnToHand(_)
            | Node::ToDeck(_)
            | Node::Exile(_)
            | Node::ToTrash(_) => true,
            Node::Apply(a) => !matches!(a.duration, Duration::ThisTurn | Duration::ThisBattle),
            Node::ApplyPlayer(a) => {
                !matches!(a.duration, Duration::ThisTurn | Duration::ThisBattle)
            }
            _ => false,
        })
}

/// Every packaged card whose Burst review is not clean, with its reasons, by card number.
#[cfg(test)]
pub(crate) fn listed_bursts(reg: &Registry) -> Vec<(String, Vec<&'static str>)> {
    let mut out = Vec::new();
    for cd in reg.db().defs() {
        let Some(aid) = reg.card(cd.def_id).burst_aid else {
            continue;
        };
        let ab = reg.ability(aid);
        let mut r = ab
            .program
            .map(|p| review_program(reg, p))
            .unwrap_or_default();
        if ab.cost_program.is_some() {
            r.cost = true;
        }
        if !r.is_clean() {
            out.push((cd.card_number.to_string(), r.reasons()));
        }
    }
    out.sort();
    out
}
