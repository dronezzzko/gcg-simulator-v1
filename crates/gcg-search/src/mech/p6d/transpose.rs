//! `transpositions` (S7, W-11 part 2): within one tree, the node of a world state is keyed by a
//! stable hash of the searcher's observation of that state, so two lines that reach the same
//! observed position share one node (and its children's statistics).
//!
//! The reference keys nodes by the action path (`_node_of(child)`, `ai/mcts.py:248-251`). The
//! key here is what the searching player can observe of the world (its information set at the
//! node): the turn, phase, step and priority; every card in a public zone with its identity,
//! rest, damage and pair, in uid order (so that two orders of deploying the same Units reach one
//! key); for every hidden zone its size and the multiset of the identities the searcher knows
//! (unknown ones as -1); the battles; the lasting and delayed effects; the counts of
//! once-per-turn records, frames and pending triggers; and the pending decision's player, kind,
//! prompt and context, with its options (as a set) only when the searcher decides (an
//! opponent's options depend on its hidden hand in that world). Worlds differ only in
//! identities hidden from the searcher, so every world that reaches the same observed position
//! reaches the same node: the single-observer tree of the reference, merged across move orders.
//! An opponent's action-step pass that the searcher cannot tell from the engine passing for it
//! leads to the same key either way, which is what `observation_keys` approximates.
//!
//! Keys are `gcg_core::stable_hash` (SHA-256) of a fixed little-endian byte encoding, never
//! `std::hash` or the process (PLAN §12.3).

use gcg_core::lookup::LookupMap;
use gcg_core::stable_hash::{StableKey, stable_key};
use gcg_engine::observe::is_hidden_from;
use gcg_engine::{GameState, HIDDEN_ZONES, Prompt, Uid, Zone};

/// The node of each observation key of one tree.
#[derive(Clone, Debug)]
pub(crate) struct Table {
    nodes: LookupMap<StableKey, u32>,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            nodes: LookupMap::new(),
        }
    }
}

impl Table {
    /// The node of `key`, if one was made.
    pub(crate) fn get(&self, key: StableKey) -> Option<u32> {
        self.nodes.get(&key).copied()
    }

    /// Records `node` as the node of `key`.
    pub(crate) fn insert(&mut self, key: StableKey, node: u32) {
        self.nodes.insert(key, node);
    }
}

/// A byte encoder of the key material.
struct Enc(Vec<u8>);

impl Enc {
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    fn len(&mut self, n: usize) {
        self.i64(i64::try_from(n).unwrap_or(i64::MAX));
    }
}

/// The searcher's view of one card identity: its definition when `searcher` knows it, else -1.
fn seen_identity(s: &GameState, uid: Uid, searcher: u8) -> i32 {
    if is_hidden_from(s, uid, searcher) {
        -1
    } else {
        i32::from(s.card(uid).def_id.0)
    }
}

/// The observation key of the world state `s` for `searcher`.
pub(crate) fn key(s: &GameState, searcher: u8) -> StableKey {
    let mut e = Enc(Vec::with_capacity(2048));
    e.i32(s.turn());
    e.u8(s.active());
    e.u8(s.phase() as u8);
    e.u8(s.step() as u8);
    e.u8(s.priority());
    e.i32(s.passes());
    for p in 0..2u8 {
        for z in Zone::ALL {
            let uids = s.zone(p, z);
            e.len(uids.len());
            if HIDDEN_ZONES.contains(&z) {
                let mut seen: Vec<i32> = uids
                    .iter()
                    .map(|&u| seen_identity(s, u, searcher))
                    .collect();
                seen.sort();
                for v in seen {
                    e.i32(v);
                }
            } else {
                let mut sorted = uids.to_vec();
                sorted.sort();
                for u in sorted {
                    let c = s.card(u);
                    e.i32(i32::from(u));
                    e.i32(seen_identity(s, u, searcher));
                    e.u8(u8::from(c.rested));
                    e.i32(c.damage);
                    e.i32(c.pair);
                }
            }
        }
    }
    e.len(s.battles().len());
    for b in s.battles() {
        for v in [
            b.attacker,
            b.attacker_seq,
            b.target,
            b.target_seq,
            b.battle_id,
        ] {
            e.i32(v);
        }
        for f in [b.blocked, b.damage_only, b.attack_triggers, b.ended] {
            e.u8(u8::from(f));
        }
        e.u8(b.defender);
    }
    for list in [s.lasting(), s.delayed()] {
        e.len(list.len());
        for l in list {
            e.u8(l.controller);
            for v in [l.source_uid, l.created_turn, l.battle_id, l.uses, l.aux] {
                e.i32(v);
            }
            e.i32(i32::from(l.player));
            let targets = s.lasting_targets(l);
            e.len(targets.len());
            for &(u, seq) in targets {
                e.i32(i32::from(u));
                e.i32(seq);
            }
        }
    }
    e.len(s.once_used().len());
    e.len(s.frames().len());
    e.len(s.pending_triggers().len());
    match s.pending() {
        None => e.u8(0),
        Some(dec) => {
            e.u8(1);
            e.u8(dec.hdr.player);
            e.u8(dec.hdr.kind as u8);
            match dec.hdr.prompt {
                Prompt::Engine(p) => {
                    e.u8(0);
                    e.u8(p as u8);
                }
                Prompt::Ask(id) => {
                    e.u8(1);
                    e.i64(i64::from(id.0));
                }
                Prompt::Choose(var) => {
                    e.u8(2);
                    e.i64(i64::from(var.0));
                }
            }
            e.len(dec.hdr.context.len());
            for &(k, v) in dec.hdr.context.as_slice() {
                e.u8(k as u8);
                e.i32(v);
            }
            if dec.hdr.player == searcher {
                let mut options: Vec<(u8, i32, i32, i32)> = dec
                    .options
                    .iter()
                    .map(|a| (a.kind as u8, a.a, a.b, a.c))
                    .collect();
                options.sort();
                e.len(options.len());
                for (kind, a, b, c) in options {
                    e.u8(kind);
                    e.i32(a);
                    e.i32(b);
                    e.i32(c);
                }
            }
        }
    }
    stable_key(&e.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gcg_engine::observe::determinize;

    #[test]
    fn keys_ignore_identities_hidden_from_the_searcher() {
        let states = crate::mech::tests::searched_states(8);
        let mut compared = 0;
        for (st, player, seed) in &states {
            let a = determinize(st, *player, *seed).expect("a world");
            let b = determinize(st, *player, seed ^ 0x5555).expect("a world");
            // Two worlds of one information set: the same key for the searcher.
            assert_eq!(key(&a, *player), key(&b, *player));
            assert_eq!(key(&a, *player), key(&a.clone(), *player));
            // The opponent sees its own hand, which differs between the worlds.
            if (0..a.cards().len()).any(|u| a.cards()[u].def_id != b.cards()[u].def_id) {
                compared += usize::from(key(&a, 1 - *player) != key(&b, 1 - *player));
            }
        }
        assert!(compared > 0);
    }

    /// `s` after `first` then `second`, when `second` is still an option of the same player's
    /// next MAIN decision.
    fn both(
        s: &GameState,
        first: gcg_engine::Action,
        second: gcg_engine::Action,
    ) -> Option<GameState> {
        let player = s.pending()?.hdr.player;
        let mut t = s.clone();
        gcg_ai::apply_unchecked(&mut t, first).ok()?;
        let dec = t.pending()?;
        if dec.hdr.player != player
            || dec.hdr.kind != gcg_engine::DecisionKind::Main
            || !dec.options.contains(&second)
        {
            return None;
        }
        gcg_ai::apply_unchecked(&mut t, second).ok()?;
        Some(t)
    }

    /// Two orders of the same two deployments reach one key (one node), which the reference's
    /// path-keyed nodes keep apart; one deployment alone reaches another key.
    #[test]
    fn two_orders_of_the_same_deployments_share_a_key() {
        let mut transposed = 0;
        for (st, player, seed) in crate::mech::tests::searched_states(usize::MAX) {
            let w = determinize(&st, player, seed).expect("a world");
            let Some(dec) = w.pending() else { continue };
            if dec.hdr.kind != gcg_engine::DecisionKind::Main {
                continue;
            }
            let plays: Vec<gcg_engine::Action> = dec
                .options
                .iter()
                .copied()
                .filter(|a| a.kind == gcg_engine::ActionKind::PlayUnit)
                .collect();
            for (i, &a) in plays.iter().enumerate() {
                for &b in &plays[i + 1..] {
                    if a.a == b.a {
                        continue;
                    }
                    let (Some(ab), Some(ba)) = (both(&w, a, b), both(&w, b, a)) else {
                        continue;
                    };
                    if key(&ab, player) == key(&ba, player) {
                        transposed += 1;
                        let mut one = w.clone();
                        gcg_ai::apply_unchecked(&mut one, a).expect("a legal play");
                        assert_ne!(key(&one, player), key(&ab, player));
                    }
                }
            }
        }
        assert!(transposed > 0);
    }
}
