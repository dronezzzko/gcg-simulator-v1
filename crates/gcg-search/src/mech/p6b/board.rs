//! The static attack step of P6-B (docs/dev/PLAN.md §12.3 S3; W-32, W-38): the board of one
//! attacking player against one defending player as numbers, with engine-correct Blockers, and
//! the exact solution of the attack step those numbers describe.
//!
//! A [`Board`] lists the attacker's Units that can attack the player (from the pending MAIN
//! decision's ATTACK options, or for a later turn every Unit with AP that no rule stops), their
//! AP, <Suppression> and <Breach>, and for each one the defender's Units that can block it:
//! `procedures::eligible_blockers` on a hypothetical battle, so <High-Maneuver>, CANT_BE_BLOCKED,
//! CANT_BLOCK, the attacker's NO_BLOCKER_VS rules and rested Blockers are the engine's own
//! (W-32). The defender's life is its Base's remaining HP and its Shields.
//!
//! [`Board::lethal`] solves the attack step exactly as a game of perfect information over these
//! numbers: the attacker picks the next attack, the defender blocks it with any eligible unused
//! Blocker or lets it through; an unblocked attack damages the Base (destroyed once its HP is
//! spent), else destroys the top Shield (two with <Suppression>), else defeats the defender; a
//! blocked <Breach> attack that destroys its Blocker deals its Breach amount to the shield area
//! (`registry.BREACH_STEPS`). Card effects, Bursts and action-step plays are not modelled: the
//! engine-backed solver (`solver.rs`) plays those out, and uses this game only to skip
//! positions where even the effect-free attack step has no lethal.

use gcg_ai::values::{hp_left, int_f64};
use gcg_core::lookup::LookupMap;
use gcg_effects::{Ev, Kw, RuleKind};
use gcg_engine::{
    Action, ActionKind, Battle, DecisionKind, Derived, EngineError, GameState, PLAYER_TARGET, Uid,
    Zone, procedures, view,
};

/// The most attackers and Blockers a [`Board`] keeps, so a mask of either fits in a `u8`
/// (`BATTLE_LIMIT` is 6).
const MAX_UNITS: usize = 8;

/// How the attackers of a [`Board`] are found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Turn {
    /// The attacker's attack step now: the Units of the ATTACK options on the player of its
    /// pending MAIN decision (none without one).
    Now,
    /// The attacker's next turn: every Unit in its Battle Area with AP above 0 and no
    /// CANT_ATTACK or CANT_ATTACK_PLAYER rule (its Units stand up at the start of that turn),
    /// against the defender's Blockers as they are now.
    Next,
    /// The rest of the attacker's turn, outside its MAIN decision (an over-approximation for
    /// the opponent's attacks during its own turn): its active Units with AP above 0 and no
    /// CANT_ATTACK or CANT_ATTACK_PLAYER rule, and the attacker of the current battle when that
    /// battle is an unblocked attack on the player.
    Rest,
}

/// One attacker of a [`Board`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Attacker {
    pub(crate) ap: i64,
    /// <High-Maneuver> or CANT_BE_BLOCKED.
    pub(crate) unblockable: bool,
    pub(crate) suppression: bool,
    pub(crate) breach: i64,
    /// The defender's Blockers (bits of indices into [`Board::blockers`]) that can block it.
    pub(crate) blockable_by: u8,
}

/// The static attack step of one player against the other.
#[derive(Clone, Debug)]
pub(crate) struct Board {
    pub(crate) attackers: Vec<Attacker>,
    /// The defender's Units that can block at least one attacker, with their remaining HP.
    pub(crate) blockers: Vec<(Uid, i64)>,
    /// The remaining HP of the defender's Base, if it has one.
    pub(crate) base: Option<i64>,
    pub(crate) shields: u32,
    /// Whether a card of the attacking player has an 【Attack】 ability (ATTACKS triggers):
    /// an attack can then change the board the static game reads (a Blocker that can't block
    /// during the battle, <High-Maneuver>, AP).
    pub(crate) attack_triggers: bool,
}

/// The battle `procedures::eligible_blockers` reads for an attack of `attacker` on the player.
fn battle_at_player(st: &GameState, attacker: Uid, defender: u8) -> Battle {
    Battle {
        attacker: i32::from(attacker),
        attacker_seq: st.card(attacker).zone_seq,
        target: PLAYER_TARGET,
        target_seq: 0,
        defender,
        battle_id: -1,
        blocked: false,
        damage_only: false,
        attack_triggers: false,
        ended: false,
    }
}

/// The defender's Units that can block an attack of `attacker` on the player, by the engine's
/// rules (`battle.eligible_blockers`, rules 8-3-1..8-3-3, 13-1-4, 13-1-6): active <Blocker>
/// Units without CANT_BLOCK that no NO_BLOCKER_VS rule of the attacker excludes, none against
/// <High-Maneuver> or CANT_BE_BLOCKED (W-32). The reference's counts (`values.ready_blockers`,
/// the evaluation's per-side Blockers) take every active <Blocker>.
///
/// # Errors
/// The engine's errors of the derived view and of filter evaluation.
pub(crate) fn effective_blockers(
    st: &GameState,
    attacker: Uid,
    defender: u8,
) -> Result<Vec<Uid>, EngineError> {
    procedures::eligible_blockers(st, &battle_at_player(st, attacker, defender))
}

/// Whether a rule of `kind` applies to `uid`.
fn has_rule(dv: &Derived, uid: Uid, kind: RuleKind) -> bool {
    view::rules_of(dv, uid, kind).next().is_some()
}

/// The attack actions on the player of a MAIN decision's options, one per attacker, in option
/// order.
pub(crate) fn player_attacks(options: &[Action]) -> Vec<Action> {
    let mut out: Vec<Action> = Vec::new();
    for &o in options {
        if o.kind == ActionKind::Attack
            && o.b == PLAYER_TARGET
            && !out.iter().any(|x: &Action| x.a == o.a)
        {
            out.push(o);
        }
    }
    out
}

/// The Units of `attacker` that can attack the player in `turn`.
fn attacking_units(st: &GameState, dv: &Derived, attacker: u8, turn: Turn) -> Vec<Uid> {
    match turn {
        Turn::Now => match st.pending() {
            Some(dec) if dec.hdr.kind == DecisionKind::Main && dec.hdr.player == attacker => {
                player_attacks(dec.options)
                    .iter()
                    .filter_map(|o| Uid::try_from(o.a).ok())
                    .collect()
            }
            _ => Vec::new(),
        },
        Turn::Next | Turn::Rest => {
            let can_attack = |u: Uid| {
                view::ap_of(st, dv, u) > 0
                    && !has_rule(dv, u, RuleKind::CantAttack)
                    && !has_rule(dv, u, RuleKind::CantAttackPlayer)
            };
            let mut out: Vec<Uid> = st
                .zone(attacker, Zone::Battle)
                .iter()
                .copied()
                .filter(|&u| can_attack(u) && (turn == Turn::Next || !st.card(u).rested))
                .collect();
            if turn == Turn::Rest
                && let Some(b) = st.battle()
                && b.target == PLAYER_TARGET
                && !b.blocked
                && let Ok(u) = Uid::try_from(b.attacker)
                && st.card(u).owner == attacker
                && !out.contains(&u)
            {
                out.push(u);
            }
            out
        }
    }
}

/// The result of one unblocked attack of `ap` on the player.
enum Hit {
    Defeats,
    Damages(Option<i64>, u32),
}

/// An unblocked attack of `ap` on a defender with a Base of `base` HP and `shields` Shields
/// (`battle.damage_step`, rule 8-5-2): the Base takes it, else the top Shield (two with
/// <Suppression>), else the defender is defeated; AP 0 deals nothing (rule 5-5-5).
fn hit(base: Option<i64>, shields: u32, ap: i64, suppression: bool) -> Hit {
    if ap <= 0 {
        return Hit::Damages(base, shields);
    }
    match base {
        Some(hp) if hp > ap => Hit::Damages(Some(hp - ap), shields),
        Some(_) => Hit::Damages(None, shields),
        None if shields > 0 => {
            Hit::Damages(None, shields.saturating_sub(1 + u32::from(suppression)))
        }
        None => Hit::Defeats,
    }
}

fn memo_key(att: u8, blk: u8, base: Option<i64>, shields: u32) -> u64 {
    let base = base.map_or(0, |hp| {
        u64::try_from(hp.clamp(0, 0x00ff_ffff)).unwrap_or(0) + 1
    });
    u64::from(att) | (u64::from(blk) << 8) | (u64::from(shields.min(0xff)) << 16) | (base << 24)
}

/// The mask of the first `n` bits.
fn mask(n: usize) -> u8 {
    u8::try_from((1u16 << n.min(8)) - 1).unwrap_or(u8::MAX)
}

impl Board {
    /// The static attack step of `attacker` in `turn`.
    ///
    /// # Errors
    /// The engine's errors of the derived view and of the Blocker rules.
    pub(crate) fn of(st: &GameState, attacker: u8, turn: Turn) -> Result<Self, EngineError> {
        let dv = view::derived(st)?;
        let defender = 1 - attacker;
        let mut units = attacking_units(st, &dv, attacker, turn);
        units.truncate(MAX_UNITS);
        let mut blockers: Vec<(Uid, i64)> = Vec::new();
        let mut attackers = Vec::with_capacity(units.len());
        for u in units {
            let mut bits = 0u8;
            for b in effective_blockers(st, u, defender)? {
                let i = match blockers.iter().position(|&(x, _)| x == b) {
                    Some(i) => i,
                    None if blockers.len() < MAX_UNITS => {
                        blockers.push((b, hp_left(st, &dv, b)));
                        blockers.len() - 1
                    }
                    None => continue,
                };
                bits |= 1 << i;
            }
            attackers.push(Attacker {
                ap: i64::from(view::ap_of(st, &dv, u)),
                unblockable: view::has_kw(&dv, u, Kw::HighManeuver)
                    || has_rule(&dv, u, RuleKind::CantBeBlocked),
                suppression: view::has_kw(&dv, u, Kw::Suppression),
                breach: i64::from(view::kw_amount(&dv, u, Kw::Breach)),
                blockable_by: bits,
            });
        }
        let base = st
            .zone(defender, Zone::Base)
            .first()
            .map(|&b| hp_left(st, &dv, b));
        let shields = u32::try_from(st.zone(defender, Zone::Shield).len()).unwrap_or(u32::MAX);
        let attack_triggers = view::trigger_index(st, &dv)
            .get(Ev::Attacks)
            .iter()
            .any(|&(host, _)| st.card(host).owner == attacker);
        Ok(Self {
            attackers,
            blockers,
            base,
            shields,
            attack_triggers,
        })
    }

    /// The upper bound of [`Board::hits_upper_bound`] with no Blocker at all.
    pub(crate) fn hits_without_blockers(&self) -> u32 {
        self.attackers
            .iter()
            .filter(|a| a.ap > 0)
            .map(|a| 1 + u32::from(a.suppression))
            .sum()
    }

    /// Whether lethal may exist once card effects are played out: the exact static game
    /// ([`Board::lethal`]), or with 【Attack】 abilities on the attacker's side the hits it can
    /// land with no Blocker at all (an 【Attack】 effect may stop a Blocker). The solver's gate:
    /// a `false` here skips the solve.
    #[must_use]
    pub(crate) fn may_be_lethal(&self) -> bool {
        if self.attack_triggers {
            self.hits_without_blockers() >= self.hits_needed()
        } else {
            self.lethal()
        }
    }

    /// The hits the defender can take before an unblocked attack defeats it: the Base (one hit
    /// at least), every Shield, then the defeating hit.
    pub(crate) fn hits_needed(&self) -> u32 {
        u32::from(self.base.is_some()) + self.shields + 1
    }

    /// An upper bound on the hits the attackers can land against the defender's best blocks:
    /// what they land when the defender blocks one feasible set of attacks (each Blocker takes
    /// the first attack it can block that no other Blocker took and that has no <Breach>). A
    /// <Suppression> attack counts two hits.
    pub(crate) fn hits_upper_bound(&self) -> u32 {
        let value = |a: &Attacker| 1 + u32::from(a.suppression);
        let mut taken = 0u8;
        for j in 0..self.blockers.len() {
            if let Some(i) = self.attackers.iter().enumerate().position(|(i, a)| {
                taken & (1 << i) == 0 && a.ap > 0 && a.breach <= 0 && a.blockable_by & (1 << j) != 0
            }) {
                taken |= 1 << i;
            }
        }
        self.attackers
            .iter()
            .enumerate()
            .filter(|&(i, a)| a.ap > 0 && taken & (1 << i) == 0)
            .map(|(_, a)| value(a))
            .sum()
    }

    /// Whether the attack step is lethal: the attacker has an order of attacks that defeats the
    /// defender whatever it blocks (the exact solution of the static game of the module docs).
    #[must_use]
    pub(crate) fn lethal(&self) -> bool {
        if self.hits_upper_bound() < self.hits_needed() {
            return false;
        }
        let mut memo = LookupMap::default();
        self.wins(
            mask(self.attackers.len()),
            mask(self.blockers.len()),
            self.base,
            self.shields,
            &mut memo,
        )
    }

    /// The attacker wins with the attackers `att` still to attack and the Blockers `blk` still
    /// unused, against a Base of `base` HP and `shields` Shields.
    fn wins(
        &self,
        att: u8,
        blk: u8,
        base: Option<i64>,
        shields: u32,
        memo: &mut LookupMap<u64, bool>,
    ) -> bool {
        if att == 0 {
            return false;
        }
        let key = memo_key(att, blk, base, shields);
        if let Some(&known) = memo.get(&key) {
            return known;
        }
        let mut result = false;
        for (i, a) in self.attackers.iter().enumerate() {
            if att & (1 << i) == 0 || a.ap <= 0 {
                continue;
            }
            let rest = att & !(1 << i);
            // The defender lets it through ...
            let through = match hit(base, shields, a.ap, a.suppression) {
                Hit::Defeats => true,
                Hit::Damages(b, s) => self.wins(rest, blk, b, s, memo),
            };
            if !through {
                continue;
            }
            // ... or blocks it with an unused eligible Blocker.
            let mut every_block_loses = true;
            for (j, &(_, hp)) in self.blockers.iter().enumerate() {
                if blk & (1 << j) == 0 || a.blockable_by & (1 << j) == 0 {
                    continue;
                }
                let breached = a.breach > 0 && a.ap >= hp && (base.is_some() || shields > 0);
                let (b, s) = match hit(base, shields, if breached { a.breach } else { 0 }, false) {
                    Hit::Damages(b, s) => (b, s),
                    Hit::Defeats => (base, shields),
                };
                if !self.wins(rest, blk & !(1 << j), b, s, memo) {
                    every_block_loses = false;
                    break;
                }
            }
            if every_block_loses {
                result = true;
                break;
            }
        }
        memo.insert(key, result);
        result
    }

    /// The AP of the attacks that connect when `blockers` Blockers each stop the largest attack
    /// they can block: the attacks no Blocker of this board can block (or, with `intrinsic`,
    /// only the <High-Maneuver> and CANT_BE_BLOCKED ones) and the blockable attacks past the
    /// `blockers` largest.
    fn through(&self, blockers: usize, intrinsic: bool) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        let mut blockable: Vec<i64> = Vec::new();
        for a in self.attackers.iter().filter(|a| a.ap > 0) {
            let free = a.unblockable || (!intrinsic && a.blockable_by == 0);
            if free {
                out.push(a.ap);
            } else {
                blockable.push(a.ap);
            }
        }
        blockable.sort_by(|x, y| y.cmp(x));
        out.extend(blockable.iter().skip(blockers));
        out
    }
}

/// Whether `attacker` has lethal in the static attack step of `turn` (the strengthened
/// `can_kill` of W-38).
///
/// # Errors
/// As [`Board::of`].
pub(crate) fn static_lethal(st: &GameState, attacker: u8, turn: Turn) -> Result<bool, EngineError> {
    Ok(Board::of(st, attacker, turn)?.lethal())
}

/// The turns `attacker` needs to win if both boards stayed as they are, capped at `cap`: the
/// strengthened `_race` of W-38 (`ai/evaluation.py:244-277` with engine-correct attackers and
/// Blockers, W-32). In the attacker's first turn only the Units that can attack then attack
/// (the ATTACK options when it is the turn player at a MAIN decision, every Unit with AP
/// otherwise) and only the defender's Blockers that are active now and can block them stop
/// one attack each (the largest); from its second turn every Unit with AP attacks and every
/// <Blocker> without CANT_BLOCK, rested now or not, stops one blockable attack. The Base
/// absorbs hits at the mean AP of the later turns' connecting attacks, then every Shield and
/// one more hit.
///
/// # Errors
/// As [`Board::of`].
pub(crate) fn race(st: &GameState, attacker: u8, cap: i64) -> Result<i64, EngineError> {
    let defender = 1 - attacker;
    let next = Board::of(st, attacker, Turn::Next)?;
    let first = if st.active() == attacker {
        Board::of(st, attacker, Turn::Now)?
    } else {
        next.clone()
    };
    let dv = view::derived(st)?;
    let later_blockers = st
        .zone(defender, Zone::Battle)
        .iter()
        .filter(|&&u| view::has_kw(&dv, u, Kw::Blocker) && !has_rule(&dv, u, RuleKind::CantBlock))
        .count();
    let through1 = first.through(first.blockers.len(), false);
    let through2 = next.through(later_blockers, true);
    let mean_of = if through2.is_empty() {
        &through1
    } else {
        &through2
    };
    if mean_of.is_empty() {
        return Ok(cap);
    }
    let count = |v: &[i64]| i64::try_from(v.len()).unwrap_or(i64::MAX);
    let mean = int_f64(mean_of.iter().sum()) / int_f64(count(mean_of));
    let base_hits = match st.zone(defender, Zone::Base).first() {
        Some(&b) => ceil_i64(int_f64(hp_left(st, &dv, b)) / mean),
        None => 0,
    };
    let shields = i64::try_from(st.zone(defender, Zone::Shield).len()).unwrap_or(i64::MAX);
    let hits = base_hits + shields + 1;
    let first_hits = count(&through1);
    if hits <= first_hits {
        return Ok(cap.min(1));
    }
    if through2.is_empty() {
        return Ok(cap);
    }
    let per_turn = count(&through2);
    Ok(cap.min(1 + (hits - first_hits + per_turn - 1) / per_turn))
}

/// `math.ceil(x)` of a finite non-negative quotient of board counts.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the quotients of board counts are small and finite, so the ceiling is exact"
)]
fn ceil_i64(x: f64) -> i64 {
    x.ceil() as i64
}
