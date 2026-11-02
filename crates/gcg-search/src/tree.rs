//! The search tree as two arenas (`_Node`, `_Child`, `ai/mcts.py:40-62`), shared by the core
//! loop (`mcts.rs`) and the mechanisms (`mech/`).

use gcg_ai::MoveKey;
use gcg_engine::DecisionKind;

/// `ChildKey = (dec.player, dec.kind.value if cfg.observation_keys else "", move_key)`
/// (`ai/mcts.py:40`, `:267-268`); `None` stands for `""`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct ChildKey {
    pub(crate) player: u8,
    pub(crate) kind: Option<DecisionKind>,
    pub(crate) key: MoveKey,
}

/// `_Child` (`ai/mcts.py:47-57`); `node` indexes [`Tree::nodes`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Child {
    pub(crate) mover: u8,
    pub(crate) prior: f64,
    pub(crate) visits: u32,
    pub(crate) total: f64,
    pub(crate) avail: u32,
    pub(crate) node: Option<u32>,
}

impl Child {
    /// `mean()` (`ai/mcts.py:56-57`).
    pub(crate) fn mean(&self) -> f64 {
        if self.visits == 0 {
            0.0
        } else {
            self.total / f64::from(self.visits)
        }
    }
}

/// The tree: `nodes[i]` is `_Node.children` (`ai/mcts.py:60-62`) as a list of
/// `(key, child index)` in insertion order, used for lookups only (never iterated for a
/// decision), and `children` holds every `_Child`.
#[derive(Default)]
pub(crate) struct Tree {
    pub(crate) nodes: Vec<Vec<(ChildKey, u32)>>,
    pub(crate) children: Vec<Child>,
}

impl Tree {
    pub(crate) fn new_node(&mut self) -> u32 {
        self.nodes.push(Vec::new());
        u32::try_from(self.nodes.len() - 1).expect("tree nodes fit in u32")
    }

    pub(crate) fn get(&self, node: u32, key: &ChildKey) -> Option<u32> {
        self.nodes[node as usize]
            .iter()
            .find_map(|(k, c)| (k == key).then_some(*c))
    }

    /// `node.children.setdefault(key, _Child(mover, prior))` (`ai/mcts.py:317`): an existing
    /// child keeps its first prior.
    pub(crate) fn setdefault(&mut self, node: u32, key: ChildKey, mover: u8, prior: f64) {
        if self.get(node, &key).is_some() {
            return;
        }
        self.children.push(Child {
            mover,
            prior,
            visits: 0,
            total: 0.0,
            avail: 0,
            node: None,
        });
        let c = u32::try_from(self.children.len() - 1).expect("tree children fit in u32");
        self.nodes[node as usize].push((key, c));
    }

    /// `_node_of(child)` (`ai/mcts.py:248-251`): the child's node, created on first use.
    pub(crate) fn node_of(&mut self, child: u32) -> u32 {
        if let Some(n) = self.children[child as usize].node {
            return n;
        }
        let n = self.new_node();
        self.children[child as usize].node = Some(n);
        n
    }
}
