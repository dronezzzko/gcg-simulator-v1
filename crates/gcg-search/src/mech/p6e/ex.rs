//! EX-payment variants at the root (`ex_variants`, W-09; docs/dev/PLAN.md §12.3 S6).
//!
//! The reference's `action_key` keeps only the play variant (`c // ALT_PLAY_BASE`) and
//! `canonical` keeps the fewest-EX action of a key (`ai/actions.py:57-85`), so a play that
//! differs only in how many EX Resources pay for it is never searched with EX. That is right
//! unless the EX payment itself does something. The *EX-sensitivity table* reads that from the
//! effect IR: a card whose own effect reads the payment (`EventAmount('ex_used')`,
//! `EventFlag('ex_used')`, the `played_with_ex` hooks of GD04-106/GD04-108 and GD05-118), a card
//! whose triggered ability listens for Commands played with EX (GD04-020, GD04-021, GD04-085),
//! and a card that triggers when an EX Resource is exiled (GD05-018). For a play of a
//! self-sensitive card, a Command while a Command listener is in play, or any paid move while an
//! exile listener is in play, the root keeps one representative per distinct EX count; every
//! other play keeps the reference's fewest-EX representative. The root prior of an EX variant
//! drops the policy's EX penalty (`EX_PENALTY`, `ai/policy.py:42`), since the reward condition
//! can be met.

use gcg_ai::{MoveKey, action_key, canonical, ex_used};
use gcg_data::DefId;
use gcg_effects::{Ev, Registry};
use gcg_engine::{Action, ActionKind, DecisionView, GameState, Uid, Zone};

/// The paid action kinds (`_PAID_KINDS`, `ai/actions.py:38`).
const PAID_KINDS: [ActionKind; 5] = [
    ActionKind::PlayUnit,
    ActionKind::PlayBase,
    ActionKind::Pair,
    ActionKind::PlayCommand,
    ActionKind::Activate,
];

/// The zones whose cards' listeners are active (`_IN_PLAY`, `ai/actions.py:202`).
const IN_PLAY: [Zone; 3] = [Zone::Battle, Zone::Base, Zone::Paired];

/// The key offset of one EX Resource in an EX variant's `variant` field. `variant` is
/// `c // ALT_PLAY_BASE`, a small play-modifier index, so the offsets never collide with it.
pub(crate) const EX_VARIANT_STEP: i32 = 1 << 16;

/// The CPython reprs of the IR nodes that read an EX payment.
const EX_READS: [&str; 4] = [
    "EventAmount(key='ex_used')",
    "EventFlag(key='ex_used'",
    "'wp_gd04_b_played_with_ex'",
    "'wp_gd05_b_played_with_ex'",
];

/// How a card's effect IR depends on EX payments.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) struct ExSense {
    /// The card's own effect reads how it was paid.
    pub(crate) own: bool,
    /// A triggered ability on Commands played reads their EX payment.
    pub(crate) commands: bool,
    /// A triggered ability fires when one of its controller's EX Resources is exiled.
    pub(crate) exiled: bool,
}

/// The EX-sensitivity of the card definition `def`, from the CPython repr of each of its
/// abilities (`Registry::repr`).
pub(crate) fn sense(reg: &Registry, def: DefId) -> ExSense {
    let entry = reg.card(def);
    let mut out = ExSense::default();
    for &aid in entry.own.iter().chain(entry.unit.iter()) {
        let ab = reg.ability(aid);
        let text = reg.repr(ab.ability.node());
        if EX_READS.iter().any(|p| text.contains(p)) {
            if ab.trigger_event == Some(Ev::CommandPlayed) {
                out.commands = true;
            } else {
                out.own = true;
            }
        }
        if ab.trigger_event == Some(Ev::Exiled) && text.contains("'ex_resource'") {
            out.exiled = true;
        }
    }
    out
}

/// `uid` as an index of `st.cards`.
fn card_index(st: &GameState, uid: i32) -> Option<Uid> {
    let u = usize::try_from(uid).ok()?;
    if u < st.cards().len() {
        Uid::try_from(u).ok()
    } else {
        None
    }
}

/// The table of the definitions met at one decision, computed once each.
struct Table<'r> {
    reg: &'r Registry,
    seen: Vec<(DefId, ExSense)>,
}

impl Table<'_> {
    fn get(&mut self, def: DefId) -> ExSense {
        if let Some(&(_, s)) = self.seen.iter().find(|(d, _)| *d == def) {
            return s;
        }
        let s = sense(self.reg, def);
        self.seen.push((def, s));
        s
    }
}

/// The root moves with EX variants: `canonical(st, dec)` (`ai/actions.py:72-85`) where the key
/// of an EX-sensitive paid action keeps its EX count, and the keys of the EX variants (EX count
/// at least 1) whose prior drops the EX penalty.
#[must_use]
pub(crate) fn canonical_with_ex(
    st: &GameState,
    dec: &DecisionView<'_>,
) -> (Vec<(MoveKey, Action)>, Vec<MoveKey>) {
    if !dec.options.iter().any(|&a| ex_used(a) > 0) {
        return (canonical(st, dec), Vec::new());
    }
    let mut table = Table {
        reg: st.registry(),
        seen: Vec::new(),
    };
    let player = dec.hdr.player;
    let mut listening = ExSense::default();
    for z in IN_PLAY {
        for &u in st.zone(player, z) {
            let s = table.get(st.card(u).def_id);
            listening.commands |= s.commands;
            listening.exiled |= s.exiled;
        }
    }
    let mut out: Vec<(MoveKey, Action)> = Vec::with_capacity(dec.options.len());
    let mut free: Vec<MoveKey> = Vec::new();
    for &action in dec.options {
        let mut key = action_key(st, dec, action);
        let ex = ex_used(action);
        let sensitive = PAID_KINDS.contains(&action.kind)
            && ex > 0
            && (listening.exiled
                || (listening.commands && action.kind == ActionKind::PlayCommand)
                || card_index(st, action.a).is_some_and(|u| table.get(st.card(u).def_id).own));
        if sensitive {
            key.variant += EX_VARIANT_STEP * ex;
        }
        match out.iter_mut().find(|(k, _)| *k == key) {
            None => {
                out.push((key, action));
                if sensitive && !free.contains(&key) {
                    free.push(key);
                }
            }
            Some(slot) => {
                if ex_used(action) < ex_used(slot.1) {
                    slot.1 = action;
                }
            }
        }
    }
    (out, free)
}
