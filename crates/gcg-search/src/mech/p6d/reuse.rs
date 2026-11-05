//! `subtree_reuse` (S7, W-11 part 3): the tree of a decision is kept for the same player's next
//! decision when the observed transition between them is the one the search predicted.
//!
//! The reference starts every decision with a fresh tree (`ai/mcts.py:127`). Here the agent
//! carries the trees of its last search (one per root tree, S1) with the move it played. At its
//! next decision the trees are reused when nothing but that move happened in between and the
//! move revealed nothing to the searcher: the same turn, the game's action counter one higher
//! (no other decision of either player, not even a forced one, was applied), and the searcher's
//! observation key (`transpose::key`) of the new information-set state equal to the key of the
//! old information-set state after the move. A move that draws or reveals a card, an engine
//! random choice, or any decision in between fails the check, so a kept tree's statistics come
//! only from worlds consistent with what the searcher knows. The new root is the node the walk
//! entered after the move: the chosen child's node, or with `transpositions` the node of the new
//! observation key. Its children keep their visits, totals, availability counts and priors; the
//! decision still runs its own budget of new iterations, and the k-th new visit of each root
//! move plays in the new decision's world k (common random numbers among the new visits).
//! Nothing here reads the true state: the carry holds information-set states' keys and trees.

use gcg_ai::apply_unchecked;
use gcg_core::stable_hash::StableKey;
use gcg_engine::{Action, GameState};

use super::transpose;
use crate::tree::Tree;

/// One kept tree: the tree and the root child of the move the agent played.
#[derive(Clone, Debug)]
pub(crate) struct Kept {
    pub(crate) tree: Tree,
    pub(crate) child: u32,
}

/// The last search of an agent and the transition it predicted.
#[derive(Clone, Debug)]
struct Last {
    player: u8,
    turn: i32,
    action_count: i64,
    /// The observation key after the played move in the old information-set state.
    expected: StableKey,
    trees: Vec<Kept>,
}

/// What a `subtree_reuse` agent carries from one decision to its next.
#[derive(Clone, Debug, Default)]
pub(crate) struct Carry {
    last: Option<Last>,
}

impl Carry {
    /// The engine applies of [`Carry::keep`] (the predicted transition), counted as auxiliary
    /// applies of the search that keeps its trees.
    pub(crate) const KEEP_APPLIES: u64 = 1;

    /// The kept trees, when `st` (the information-set state of `player`'s new decision) is the
    /// state the kept search's move led to; the carry is empty afterwards either way.
    pub(crate) fn take_for(&mut self, st: &GameState, player: u8) -> Option<Vec<Kept>> {
        let last = self.last.take()?;
        (last.player == player
            && last.turn == st.turn()
            && st.action_count() == last.action_count + 1
            && transpose::key(st, player) == last.expected)
            .then_some(last.trees)
    }

    /// Keeps `trees` after `player` chose `action` at the information-set state `st`, when the
    /// move leads straight to another decision of `player` there.
    pub(crate) fn keep(&mut self, st: &GameState, player: u8, action: Action, trees: Vec<Kept>) {
        let mut after = st.clone();
        self.last = match apply_unchecked(&mut after, action) {
            Ok(()) if after.pending().is_some_and(|d| d.hdr.player == player) => Some(Last {
                player,
                turn: st.turn(),
                action_count: st.action_count(),
                expected: transpose::key(&after, player),
                trees,
            }),
            _ => None,
        };
    }

    /// Forgets the kept trees (a decision without a search).
    pub(crate) fn clear(&mut self) {
        self.last = None;
    }
}

/// The root node of `kept` for the new decision at `st`: the node of the new observation key
/// with `transpositions`, else the node of the played child; `None` when the walk never
/// entered one.
pub(crate) fn root_of(
    kept: &Kept,
    st: &GameState,
    player: u8,
    transpositions: bool,
) -> Option<u32> {
    if transpositions {
        kept.tree.p6d.table.get(transpose::key(st, player))
    } else {
        kept.tree.children.get(kept.child as usize)?.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gcg_ai::canonical;
    use gcg_engine::observe::determinize;

    fn kept() -> Vec<Kept> {
        vec![Kept {
            tree: Tree::default(),
            child: 7,
        }]
    }

    /// A kept tree is handed over only at the decision its move predicted, once.
    #[test]
    fn kept_trees_go_only_to_the_predicted_decision() {
        let mut checked = 0;
        for (st, player, seed) in crate::mech::tests::searched_states(usize::MAX) {
            let info = determinize(&st, player, seed).expect("a world");
            let Some(dec) = info.pending() else { continue };
            for (_, a) in canonical(&info, &dec) {
                let mut after = info.clone();
                if apply_unchecked(&mut after, a).is_err()
                    || !after.pending().is_some_and(|d| d.hdr.player == player)
                    || after.turn() != info.turn()
                {
                    continue;
                }
                let mut carry = Carry::default();
                carry.keep(&info, player, a, kept());
                let got = carry
                    .take_for(&after, player)
                    .expect("the predicted decision");
                assert_eq!(got[0].child, 7);
                assert!(carry.take_for(&after, player).is_none());
                // Another player's decision, or the old decision again, gets nothing.
                carry.keep(&info, player, a, kept());
                assert!(carry.take_for(&after, 1 - player).is_none());
                carry.keep(&info, player, a, kept());
                assert!(carry.take_for(&info, player).is_none());
                checked += 1;
                break;
            }
        }
        assert!(checked > 0);
    }
}
