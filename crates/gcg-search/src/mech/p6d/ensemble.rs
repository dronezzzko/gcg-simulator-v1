//! `root_trees{K}` (S1, W-04): a deterministic root ensemble of K trees per decision.
//!
//! After the root moves are chosen (one shared list, so every tree searches the same moves),
//! one draw of the per-decision generator gives the decision seed. Tree `t` is seeded with
//! `derive_seed(decision_seed, "tree", t)` and draws its own world and rollout seeds from that
//! stream in the reference order (`W[0:b]`, `R[0:b]`, `W[b:cap]`, `R[b:cap]`), so the trees
//! play disjoint worlds and rollouts. Each tree gets `ceil(B/K)` iterations and an extension
//! cap of `ceil(cap/K)`, and keeps the reference's per-tree early stop (`_settled`). The trees
//! run as one `gcg_core::pool` fork-join per phase; every job is a pure function of its inputs,
//! and the results come back in tree order, so the decision is identical for any worker count.
//! Root statistics merge in tree order: visits summed, totals summed with `py_sum`; the move is
//! the first maximum of `(visits, mean)`. The close-call test of an effect-target decision
//! (`_close_call`, `ai/mcts.py:181-202`) uses the concatenation, in tree order, of each tree's
//! paired per-visit differences (the k-th visits of two moves pair only inside one tree), and an
//! extension adds one per-tree budget to every tree.

use gcg_core::derive_seed;
use gcg_core::pymath::py_sum;

use crate::mech::p6o::math::Math;
use crate::mech::reference::py_max;

/// `TIE_TOLERANCE` (`ai/mcts.py:41`).
const TIE_TOLERANCE: f64 = 1e-9;

/// The seed of tree `t` of a decision.
pub(crate) fn tree_seed(decision_seed: u64, t: usize) -> u64 {
    derive_seed!(decision_seed, "tree", t)
}

/// `ceil(n / k)` for `k >= 1`.
pub(crate) const fn per_tree(n: u32, k: u32) -> u32 {
    n.div_ceil(k)
}

/// One root move's merged statistics: summed visits and the `py_sum` of the trees' totals.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Merged {
    pub(crate) visits: u32,
    pub(crate) total: f64,
}

impl Merged {
    pub(crate) fn mean(self) -> f64 {
        if self.visits == 0 {
            0.0
        } else {
            self.total / f64::from(self.visits)
        }
    }
}

/// Merges `stats[t][i] = (visits, total)` of root move `i` in tree `t`, in tree order.
pub(crate) fn merge(stats: &[Vec<(u32, f64)>]) -> Vec<Merged> {
    let n = stats.first().map_or(0, Vec::len);
    (0..n)
        .map(|i| Merged {
            visits: stats.iter().map(|s| s[i].0).sum(),
            total: py_sum(stats.iter().map(|s| s[i].1)),
        })
        .collect()
}

/// The first index of the greatest `(visits, mean)` (CPython's `max` keeps the first).
pub(crate) fn leader(merged: &[Merged]) -> usize {
    let mut best = 0;
    for (i, m) in merged.iter().enumerate().skip(1) {
        let b = merged[best];
        if m.visits > b.visits || (m.visits == b.visits && m.mean() > b.mean()) {
            best = i;
        }
    }
    best
}

/// A Python `int` count converted to a float (exact for every count in scope).
fn count_f64(n: usize) -> f64 {
    f64::from(u32::try_from(n).expect("counts in scope fit in u32"))
}

/// `_close_call` over the trees: `per_visit[t][i]` are tree `t`'s per-visit value differences
/// of root move `i`. The leader is the merged one; against every other move visited in some
/// tree, the paired differences are each tree's `lead[j] - other[j]` for `j` below both
/// lengths, concatenated in tree order. With one tree this is the reference test.
pub(crate) fn close_call(merged: &[Merged], per_visit: &[&[Vec<f64>]], z: f64, math: Math) -> bool {
    let lead = leader(merged);
    for i in 0..merged.len() {
        if i == lead || per_visit.iter().all(|pv| pv[i].is_empty()) {
            continue;
        }
        let mut diffs: Vec<f64> = Vec::new();
        for pv in per_visit {
            let (a, b) = (&pv[lead], &pv[i]);
            diffs.extend((0..a.len().min(b.len())).map(|j| a[j] - b[j]));
        }
        let n = diffs.len();
        if n < 2 {
            return true;
        }
        let abs: Vec<f64> = diffs.iter().map(|x| x.abs()).collect();
        if py_max(&abs) <= TIE_TOLERANCE {
            continue;
        }
        let n_f = count_f64(n);
        let mean = py_sum(diffs.iter().copied()) / n_f;
        let var = py_sum(diffs.iter().map(|&x| math.square(x - mean))) / count_f64(n - 1);
        if mean <= z * math.sqrt(var / n_f) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_split_by_ceiling() {
        assert_eq!(per_tree(640, 4), 160);
        assert_eq!(per_tree(641, 4), 161);
        assert_eq!(per_tree(160, 3), 54);
        assert_eq!(per_tree(7, 1), 7);
        assert_eq!(per_tree(0, 8), 0);
    }

    #[test]
    fn tree_seeds_are_distinct_streams_of_the_decision_seed() {
        let seeds: Vec<u64> = (0..10).map(|t| tree_seed(12345, t)).collect();
        for (i, s) in seeds.iter().enumerate() {
            assert!(seeds[i + 1..].iter().all(|o| o != s));
        }
        assert_eq!(tree_seed(12345, 3), derive_seed!(12345u64, "tree", 3usize));
        assert_ne!(tree_seed(12346, 3), tree_seed(12345, 3));
    }

    #[test]
    fn root_statistics_merge_in_tree_order() {
        let stats = vec![
            vec![(3, 1.5), (2, 1.8), (0, 0.0)],
            vec![(2, 0.2), (3, 0.9), (1, 0.4)],
        ];
        let m = merge(&stats);
        assert_eq!(
            m[0],
            Merged {
                visits: 5,
                total: py_sum([1.5, 0.2])
            }
        );
        assert_eq!(m[1].visits, 5);
        // Equal visits: the higher mean leads (0.54 against 0.34).
        assert_eq!(leader(&m), 1);
        assert_eq!(leader(&merge(&[vec![(2, 1.0), (2, 1.0)]])), 0);
    }

    #[test]
    fn the_close_call_pairs_visits_only_inside_a_tree() {
        let math = Math::Platform;
        let merged = [
            Merged {
                visits: 4,
                total: 3.2,
            },
            Merged {
                visits: 4,
                total: 1.6,
            },
        ];
        // Inside each tree the leader is clearly better: no close call.
        let t0 = vec![vec![0.8, 0.8], vec![0.4, 0.4]];
        let t1 = vec![vec![0.8, 0.8], vec![0.4, 0.4]];
        assert!(!close_call(&merged, &[&t0, &t1], 2.0, math));
        // One shared visit per tree: two pairs in total, not one.
        let t0 = vec![vec![0.9], vec![0.3]];
        let t1 = vec![vec![0.7], vec![0.5]];
        assert!(!close_call(&merged, &[&t0, &t1], 0.5, math));
        // A move no tree visited is ignored.
        let unvisited = vec![vec![0.9, 0.2], vec![]];
        assert!(!close_call(&merged, &[&unvisited, &unvisited], 2.0, math));
        // A single tree with one visit of the other move is a close call, as the reference.
        let single = vec![vec![0.9, 0.9], vec![0.3]];
        assert!(close_call(&merged, &[&single], 2.0, math));
    }
}
