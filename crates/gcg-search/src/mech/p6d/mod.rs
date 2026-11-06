//! P6-D's tree and budget mechanisms (docs/dev/PLAN.md §12.2, §12.3 S1, S4, S7, S8):
//!
//! - `root_trees` (S1, W-04): a deterministic root ensemble, `ensemble.rs`, run by the core
//!   loop (`mcts.rs`) through `gcg_core::pool` fork-join;
//! - `visited_set_backup` (W-06): each tree child is updated at most once per iteration,
//!   `backup.rs`;
//! - `in_tree_filters` (W-11): the cheap, honest root filters for the mover at every tree node,
//!   `filters.rs`;
//! - `transpositions` (S7, W-11): tree nodes keyed by a stable hash of the searcher's
//!   observation of the world state, `transpose.rs`;
//! - `macro_multi_pick` (S8, W-138): a small multi-card choice as one move per subset,
//!   `macros.rs`;
//! - `prior_worlds` (W-140): in-tree priors averaged over the first K worlds that reach a node,
//!   `priors.rs`;
//! - `subtree_reuse` (S7, W-11): the tree of a decision kept for the same player's next decision
//!   when the observed transition is the predicted one, `reuse.rs` (the agent carries it).
//!
//! Every mechanism is a function of the configuration, the information-set state, the worlds
//! determinised from it and the per-decision generator, so its results are identical for any
//! worker count (nothing reads a clock, a thread id or a process hash).

pub(crate) mod backup;
pub(crate) mod ensemble;
pub(crate) mod filters;
pub(crate) mod macros;
pub(crate) mod priors;
pub(crate) mod reuse;
pub(crate) mod transpose;

/// The mechanism flags this package implements.
pub(crate) const PROVIDES: [&str; 7] = [
    "in_tree_filters",
    "macro_multi_pick",
    "prior_worlds",
    "root_trees",
    "subtree_reuse",
    "transpositions",
    "visited_set_backup",
];

/// The per-tree tables of the P6-D mechanisms, kept in [`crate::tree::Tree`].
#[derive(Clone, Debug, Default)]
pub(crate) struct TreeExt {
    /// `transpositions`: the node of each observation key.
    pub(crate) table: transpose::Table,
    /// `prior_worlds`: the policy scores each node has seen, per world.
    pub(crate) priors: priors::Records,
}
