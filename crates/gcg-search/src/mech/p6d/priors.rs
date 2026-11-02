//! `prior_worlds{K}` (W-140): an in-tree node's prior for each move averages the policy scores
//! over the first K worlds that reach the node.
//!
//! The reference scores a node's moves once, in the world of the iteration that first adds its
//! children, and `setdefault` keeps those priors when a later world brings a move the node has
//! not seen (`_add_children`, `ai/mcts.py:305-317`), so one sampled hand can set an opponent
//! node's priors and a node's priors mix softmaxes over different move sets. Here every node
//! keeps the scores of each world that reached it (by the iteration's world index, the
//! common-random-number world of the root, `ai/mcts.py:136-145`): the first K distinct worlds
//! contribute every move's score, and a later world contributes only the scores of moves not
//! seen before. A move's prior is the softmax (temperature `prior_temperature`) of its mean
//! score, normalised over every move seen for the same decider and decision kind, and the
//! priors are recomputed whenever a contributing world or a new move arrives. Scores are summed
//! in world order and the softmax is taken in a fixed key order, so the priors do not depend on
//! the order in which the same worlds arrive. The root prior is computed once, as the
//! reference's (the core never refreshes the root). The scores are `policy.option_scores`, which
//! apply nothing.

use gcg_core::pymath::py_sum;

use super::super::NodeCx;
use super::super::reference::py_max;
use crate::tree::ChildKey;

/// The scores one node has seen.
#[derive(Clone, Debug, Default)]
struct Record {
    /// The worlds that contributed every move's score, ascending.
    worlds: Vec<usize>,
    /// Each move seen, in first-seen order, with its `(world, score)` pairs in world order.
    entries: Vec<(ChildKey, Vec<(usize, f64)>)>,
}

/// The records of every node of one tree, indexed by node.
#[derive(Clone, Debug, Default)]
pub(crate) struct Records {
    per_node: Vec<Option<Record>>,
}

impl Records {
    fn get(&self, node: u32) -> Option<&Record> {
        self.per_node.get(node as usize).and_then(Option::as_ref)
    }

    fn get_mut(&mut self, node: u32) -> &mut Record {
        let i = node as usize;
        if self.per_node.len() <= i {
            self.per_node.resize_with(i + 1, || None);
        }
        self.per_node[i].get_or_insert_with(Record::default)
    }
}

/// Whether the node takes the scores of world `world`: fewer than `k` worlds contributed so far
/// and this one is new.
pub(crate) fn wants(records: &Records, node: u32, world: usize, k: u32) -> bool {
    records.get(node).is_none_or(|r| {
        r.worlds.len() < k.max(1) as usize && r.worlds.binary_search(&world).is_err()
    })
}

/// A fixed total order of child keys: decider, decision kind, then the move key's fields.
fn order(k: &ChildKey) -> (u8, u8, u8, i64, i64, i32) {
    (
        k.player,
        k.kind.map_or(u8::MAX, |d| d as u8),
        k.key.kind as u8,
        k.key.a,
        k.key.b,
        k.key.variant,
    )
}

/// The priors hook under `prior_worlds{k}`: records the node's scores in this world (every move
/// for one of the first `k` worlds, otherwise only new moves) and sets every child's prior to
/// the softmax of its mean score within its decider and decision kind. `scores` are the policy
/// scores of the node's moves in `cx.moves`' order (`policy.option_scores`, or with a P6-F flag
/// on the strengthened policy's).
pub(crate) fn priors(cx: &mut NodeCx<'_>, k: u32, scores: &[f64]) {
    let world = cx.world;
    let rec = cx.tree.p6d.priors.get_mut(cx.node);
    let contributes = rec.worlds.len() < k.max(1) as usize;
    let full = match rec.worlds.binary_search(&world) {
        Err(at) if contributes => {
            rec.worlds.insert(at, world);
            true
        }
        _ => false,
    };
    for (&key, &score) in cx.keys.iter().zip(scores) {
        match rec.entries.iter_mut().find(|(k, _)| *k == key) {
            Some((_, seen)) => {
                if full && let Err(at) = seen.binary_search_by(|&(w, _)| w.cmp(&world)) {
                    seen.insert(at, (world, score));
                }
            }
            None => rec.entries.push((key, vec![(world, score)])),
        }
    }
    let mut ordered: Vec<(ChildKey, f64)> = rec
        .entries
        .iter()
        .map(|(key, seen)| {
            let sum = py_sum(seen.iter().map(|&(_, s)| s));
            (*key, sum / count_f64(seen.len()))
        })
        .collect();
    ordered.sort_by_key(|(key, _)| order(key));
    let mut start = 0;
    while start < ordered.len() {
        let group = |k: &ChildKey| (k.player, k.kind);
        let id = group(&ordered[start].0);
        let end = start
            + ordered[start..]
                .iter()
                .take_while(|(k, _)| group(k) == id)
                .count();
        let means: Vec<f64> = ordered[start..end].iter().map(|&(_, m)| m).collect();
        let top = py_max(&means);
        let weights: Vec<f64> = means
            .iter()
            .map(|&x| cx.math.exp((x - top) / cx.cfg.prior_temperature))
            .collect();
        let total = py_sum(weights.iter().copied());
        for (&(key, _), &w) in ordered[start..end].iter().zip(&weights) {
            cx.tree.set_prior(cx.node, key, key.player, w / total);
        }
        start = end;
    }
}

/// A count as a float (exact for every count in scope).
fn count_f64(n: usize) -> f64 {
    f64::from(u32::try_from(n).expect("counts in scope fit in u32"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mech::p6o::math::Math;
    use crate::mech::tests::searched_states;
    use crate::tree::Tree;
    use crate::{Preset, preset_config};
    use gcg_ai::{canonical, option_scores};
    use gcg_engine::observe::determinize;
    use gcg_engine::{Action, GameState};

    /// The priors of node 0 after the worlds `order` (indices into `worlds`) reach it.
    fn priors_after(st: &GameState, player: u8, worlds: &[GameState], order: &[usize]) -> Vec<u64> {
        let cfg = preset_config(Preset::Fast);
        let mut tree = Tree::default();
        let node = tree.new_node();
        let mut all: Vec<ChildKey> = Vec::new();
        for &w in order {
            let s = &worlds[w];
            let dec = s.pending().expect("a decision");
            let moves = canonical(s, &dec);
            let keys: Vec<ChildKey> = moves
                .iter()
                .map(|&(key, _)| ChildKey {
                    player: dec.hdr.player,
                    kind: Some(dec.hdr.kind),
                    key,
                })
                .collect();
            for k in &keys {
                if !all.contains(k) {
                    all.push(*k);
                }
            }
            let options: Vec<Action> = moves.iter().map(|&(_, a)| a).collect();
            let scores = option_scores(s, &dec, &options, &cfg.knobs()).expect("scores");
            priors(
                &mut NodeCx {
                    tree: &mut tree,
                    node,
                    s,
                    dec: &dec,
                    moves: &moves,
                    keys: &keys,
                    children: &[],
                    cfg,
                    knobs: cfg.knobs(),
                    math: Math::Platform,
                    world: w,
                },
                4,
                &scores,
            );
        }
        let _ = (st, player);
        all.sort_by_key(order_of);
        all.iter()
            .map(|k| {
                let c = tree.get(node, k).expect("a child per move seen");
                tree.children[c as usize].prior.to_bits()
            })
            .collect()
    }

    fn order_of(k: &ChildKey) -> (u8, u8, u8, i64, i64, i32) {
        order(k)
    }

    #[test]
    fn priors_do_not_depend_on_the_order_the_same_worlds_arrive_in() {
        let mut checked = 0;
        for (st, player, seed) in searched_states(24) {
            let worlds: Vec<GameState> = (0..4u64)
                .map(|i| determinize(&st, player, seed.wrapping_add(i)).expect("a world"))
                .collect();
            let a = priors_after(&st, player, &worlds, &[0, 1, 2, 3]);
            let b = priors_after(&st, player, &worlds, &[3, 1, 0, 2]);
            let c = priors_after(&st, player, &worlds, &[2, 3, 1, 0]);
            assert_eq!(a, b);
            assert_eq!(a, c);
            let total: f64 = a.iter().map(|&p| f64::from_bits(p)).fold(0.0, |x, y| x + y);
            assert!((total - 1.0).abs() < 1e-9, "{total}");
            checked += 1;
        }
        assert!(checked > 0);
    }

    #[test]
    fn only_the_first_k_worlds_contribute_every_score() {
        let mut records = Records::default();
        assert!(wants(&records, 3, 7, 2));
        let rec = records.get_mut(3);
        rec.worlds = vec![1, 7];
        assert!(!wants(&records, 3, 9, 2));
        assert!(!wants(&records, 3, 7, 4));
        assert!(wants(&records, 3, 9, 4));
        assert!(wants(&records, 2, 0, 1));
    }
}
