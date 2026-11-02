//! `visited_set_backup` (W-06): the backup of one line through a per-iteration visited set, so
//! that a child the walk reached twice in one iteration gains one visit, not two.
//!
//! The reference (`ai/mcts.py:214-217`) updates every entry of the path. With observation keys
//! an opponent's action-step pass keeps the walk in the same node (`_unobserved_pass`,
//! `ai/mcts.py:238-245`), so when the opponent passes in a battle's action step and again in the
//! end phase's action step with no multi-move decision between, both passes select the same
//! child and the reference counts that iteration twice for it (`docs/AI.md:469-474`). A cycle
//! through a transposed node (`transpositions`) repeats a child the same way.

use super::super::BackupCx;

/// `ai/mcts.py:214-217` over the distinct children of the path, in first-visit order: each
/// gains one visit and the value from its mover's point of view.
pub(crate) fn backup(cx: &mut BackupCx<'_>) {
    for (i, &c) in cx.path.iter().enumerate() {
        if cx.path[..i].contains(&c) {
            continue;
        }
        let child = &mut cx.tree.children[c as usize];
        child.visits += 1;
        child.total += if child.mover == cx.player {
            cx.value
        } else {
            1.0 - cx.value
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mech::reference;
    use crate::tree::{ChildKey, Tree};
    use gcg_ai::MoveKey;
    use gcg_engine::{ActionKind, DecisionKind};

    /// A root child (the searcher's move), then the opponent's pass child twice: the battle's
    /// action-step pass and the end phase's action-step pass of the same node.
    fn scripted() -> (Tree, [u32; 2]) {
        let mut tree = Tree::default();
        let root = tree.new_node();
        let key = |player, kind, a| ChildKey {
            player,
            kind: Some(kind),
            key: MoveKey {
                kind: a,
                a: -1,
                b: -1,
                variant: -1,
            },
        };
        tree.setdefault(root, key(0, DecisionKind::Main, ActionKind::Attack), 0, 0.5);
        let first = tree.get(root, &key(0, DecisionKind::Main, ActionKind::Attack));
        let first = first.expect("the root child");
        let node = tree.node_of(first);
        let pass = key(1, DecisionKind::ActionStep, ActionKind::Pass);
        tree.setdefault(node, pass, 1, 1.0);
        let pass = tree.get(node, &pass).expect("the pass child");
        (tree, [first, pass])
    }

    #[test]
    fn a_child_reached_twice_gains_one_visit_per_iteration() {
        for iterations in 1..4u32 {
            let (mut visited, [first, pass]) = scripted();
            let (mut reference_tree, _) = scripted();
            for _ in 0..iterations {
                let path = [first, pass, pass];
                backup(&mut BackupCx {
                    tree: &mut visited,
                    path: &path,
                    value: 0.75,
                    player: 0,
                });
                reference::backup(&mut BackupCx {
                    tree: &mut reference_tree,
                    path: &path,
                    value: 0.75,
                    player: 0,
                });
            }
            let shared = &visited.children[pass as usize];
            assert_eq!(shared.visits, iterations);
            assert_eq!(
                shared.total.to_bits(),
                (0.25 * f64::from(iterations)).to_bits()
            );
            // The reference (pinned): the shared node's count grows by 2 per iteration.
            let pinned = &reference_tree.children[pass as usize];
            assert_eq!(pinned.visits, 2 * iterations);
            assert_eq!(visited.children[first as usize].visits, iterations);
            assert_eq!(reference_tree.children[first as usize].visits, iterations);
        }
    }

    #[test]
    fn a_path_without_repeats_backs_up_as_the_reference() {
        let (mut visited, [first, pass]) = scripted();
        let (mut reference_tree, _) = scripted();
        for value in [0.1, 0.9, 0.35] {
            let path = [first, pass];
            backup(&mut BackupCx {
                tree: &mut visited,
                path: &path,
                value,
                player: 0,
            });
            reference::backup(&mut BackupCx {
                tree: &mut reference_tree,
                path: &path,
                value,
                player: 0,
            });
        }
        for c in [first, pass] {
            let (a, b) = (
                &visited.children[c as usize],
                &reference_tree.children[c as usize],
            );
            assert_eq!((a.visits, a.total.to_bits()), (b.visits, b.total.to_bits()));
        }
    }

    /// The test mechanisms of the search-level check: `Composed`'s backup, recording for every
    /// child the path repeats its key and how many visits the backup gave it.
    struct Recording<'c> {
        inner: crate::mech::Composed<'c>,
        repeats: std::cell::RefCell<Vec<(ChildKey, u32)>>,
    }

    impl crate::mech::Mechanisms for Recording<'_> {
        fn backup(&self, cx: &mut BackupCx<'_>) {
            let repeated: Vec<u32> = cx
                .path
                .iter()
                .enumerate()
                .filter(|&(i, c)| cx.path[..i].contains(c) && !cx.path[i + 1..].contains(c))
                .map(|(_, &c)| c)
                .collect();
            let before: Vec<u32> = repeated
                .iter()
                .map(|&c| cx.tree.children[c as usize].visits)
                .collect();
            crate::mech::Mechanisms::backup(&self.inner, cx);
            for (&c, b) in repeated.iter().zip(before) {
                let key = cx
                    .tree
                    .nodes
                    .iter()
                    .flatten()
                    .find_map(|&(k, child)| (child == c).then_some(k))
                    .expect("every child has a key");
                let gained = cx.tree.children[c as usize].visits - b;
                self.repeats.borrow_mut().push((key, gained));
            }
        }
    }

    /// The repeated children of every iteration of the searches of the vector decisions.
    fn repeats(flags: &[&str]) -> Vec<(ChildKey, u32)> {
        let cfg = crate::mech::tests::config(flags);
        let mech = Recording {
            inner: crate::mech::Composed::new(&cfg),
            repeats: std::cell::RefCell::new(Vec::new()),
        };
        for (st, player, seed) in crate::mech::tests::searched_states(usize::MAX) {
            crate::mech::tests::run_with(&mech, &st, player, &cfg, seed).expect("a search");
        }
        mech.repeats.into_inner()
    }

    /// W-06 at search level: in the searches of the D2 vector decisions, every child a walk
    /// reached twice in one iteration is an opponent's action-step pass; the reference backs it
    /// up twice (pinned), the visited set once.
    #[test]
    fn a_repeated_opponent_pass_is_backed_up_once_with_the_visited_set() {
        let reference = repeats(&[]);
        assert!(!reference.is_empty());
        for &(key, gained) in &reference {
            assert_eq!(gained, 2, "{key:?}");
            assert_eq!(key.kind, Some(DecisionKind::ActionStep), "{key:?}");
            assert_eq!(key.key.kind, ActionKind::Pass, "{key:?}");
        }
        let visited = repeats(&["visited_set_backup"]);
        assert!(!visited.is_empty());
        for &(key, gained) in &visited {
            assert_eq!(gained, 1, "{key:?}");
        }
        println!(
            "W-06: {} repeated children in the reference, {} with the visited set",
            reference.len(),
            visited.len()
        );
    }
}
