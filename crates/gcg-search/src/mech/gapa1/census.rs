//! W-05: the census of observation-key splits (the Rust version of the investigation's 183-position
//! census, `docs/python/AI.md:462-469`, of which only the counts survive).
//!
//! One search runs at a position with a recording tracer. Every iteration's walk is replayed
//! from its world (`SearchTracer::iteration` gets the world, `SearchTracer::apply` the walk's
//! actions) and the tree node of every decision in the walk is reconstructed exactly as the core
//! loop finds it (`mcts.rs`, `descend`): the root move's child, then at every decision with more
//! than one canonical move the child of the move taken, keyed `(player, kind if
//! observation_keys, move key)` (`_child_key`, `ai/mcts.py:267-268`), except that with
//! `observation_keys` an opponent's action-step pass keeps the node (`_unobserved_pass`,
//! `ai/mcts.py:238-245`). A node is its key path from the root.
//!
//! At every node where the searcher decides (below the root), the searcher's observation of the
//! world is keyed as `transpositions` keys it (`p6d::transpose::key`: what the searcher can
//! observe, its options included). A *split* is one more node for an observation key that
//! already has a node: the searcher holds one information set there, but the tree keeps
//! separate statistics, because the paths differ by something the searcher could not see or by
//! the order of moves. Each split is classified by the first difference of its path from the
//! key's first path:
//!
//! - `opponent_pass_vs_no_decision`: one path has an opponent's action-step pass node that the
//!   other lacks (the opponent had a play in one world and none in the other; the auto-pass
//!   split that `observation_keys` removes);
//! - `move_order`: the paths hold the same moves in another order (a transposition);
//! - `extra_node:<mover>:<kind>`: one path is the other plus nodes whose moves left the
//!   searcher's observation unchanged;
//! - `<mover>_move:<kind>`: the paths take different moves at a decision of that kind.
//!
//! With `transpositions` the core keys every node by this same observation key, so no split is
//! left by construction (`mcts.rs`, `descend`), and the census records each key's one node; the
//! census measures what the reference's action-path keys leave. Configurations with
//! `macro_multi_pick` or `in_tree_filters` (other node moves) are refused.

use gcg_ai::{
    AgentCx, AgentError, IterTrace, MoveKey, SearchResult, SearchTracer, apply_unchecked, canonical,
};
use gcg_core::lookup::LookupMap;
use gcg_core::rng::SplitMix64;
use gcg_core::stable_hash::StableKey;
use gcg_engine::{Action, ActionKind, DecisionKind, EngineError, GameState};

use crate::mech::p6d::transpose;
use crate::tree::ChildKey;
use crate::{Observation, StrengthenedConfig};

/// One step of a reconstructed path: the child key, and the decision kind and action behind it
/// (for the classification; the key alone carries the kind only with `observation_keys`).
#[derive(Clone, Copy, Debug)]
struct Step {
    key: ChildKey,
    kind: DecisionKind,
    action: Action,
}

impl PartialEq for Step {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

/// The census of one search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplitCensus {
    /// The search's iterations.
    pub iterations: u32,
    /// Distinct searcher nodes below the root that the walks entered.
    pub searcher_nodes: u32,
    /// Distinct observation keys of those nodes.
    pub observation_keys: u32,
    /// Nodes beyond the first of their observation key.
    pub splits: u32,
    /// The splits by cause (module documentation), in order of first appearance.
    pub causes: Vec<(String, u32)>,
}

impl SplitCensus {
    fn count(&mut self, cause: String) {
        match self.causes.iter_mut().find(|(c, _)| *c == cause) {
            Some((_, n)) => *n += 1,
            None => self.causes.push((cause, 1)),
        }
    }
}

/// The recording tracer: the actions of the running iteration, and the nodes found so far.
struct Recorder<'c> {
    searcher: u8,
    cfg: &'c StrengthenedConfig,
    root_moves: Vec<(MoveKey, Action)>,
    actions: Vec<Action>,
    /// Observation key to its index in `paths`.
    index: LookupMap<StableKey, usize>,
    paths: Vec<Vec<Vec<Step>>>,
    error: Option<EngineError>,
}

fn mover(player: u8, searcher: u8) -> &'static str {
    if player == searcher {
        "own"
    } else {
        "opponent"
    }
}

fn is_opponent_pass(s: &Step, searcher: u8) -> bool {
    s.key.player != searcher
        && s.kind == DecisionKind::ActionStep
        && s.action.kind == ActionKind::Pass
}

/// The cause of the split of path `b` from the first path `a` of its key.
fn cause(a: &[Step], b: &[Step], searcher: u8) -> String {
    let i = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or_else(|| a.len().min(b.len()));
    let without = |p: &[Step], j: usize| -> Vec<Step> {
        p.iter()
            .enumerate()
            .filter(|&(k, _)| k != j)
            .map(|(_, s)| *s)
            .collect()
    };
    for (p, q) in [(a, b), (b, a)] {
        if i < p.len() && is_opponent_pass(&p[i], searcher) && without(p, i).as_slice() == q {
            return "opponent_pass_vs_no_decision".to_owned();
        }
    }
    if i >= a.len().min(b.len()) {
        let longer = if a.len() > b.len() { a } else { b };
        let s = longer[i];
        return format!(
            "extra_node:{}:{}",
            mover(s.key.player, searcher),
            s.kind.as_str()
        );
    }
    let sorted = |p: &[Step]| -> Vec<(u8, &'static str, i64, i64, i32)> {
        let mut v: Vec<(u8, &'static str, i64, i64, i32)> = p
            .iter()
            .map(|s| {
                (
                    s.key.player,
                    s.key.key.kind.as_str(),
                    s.key.key.a,
                    s.key.key.b,
                    s.key.key.variant,
                )
            })
            .collect();
        v.sort();
        v
    };
    if a.len() == b.len() && sorted(a) == sorted(b) {
        return "move_order".to_owned();
    }
    format!(
        "{}_move:{}",
        mover(a[i].key.player, searcher),
        a[i].kind.as_str()
    )
}

impl Recorder<'_> {
    fn child_key(&self, player: u8, kind: DecisionKind, key: MoveKey) -> ChildKey {
        ChildKey {
            player,
            kind: self.cfg.observation_keys.then_some(kind),
            key,
        }
    }

    /// Replays one walk from `world` and records the searcher's nodes on it.
    fn replay(&mut self, world: &GameState, walk: &[Action]) -> Result<(), EngineError> {
        let mut s = world.clone();
        let mut path: Vec<Step> = Vec::new();
        for (j, &a) in walk.iter().enumerate() {
            let dec = s
                .pending()
                .ok_or_else(|| EngineError::Assertion("a walk move without a decision".into()))?;
            let (player, kind) = (dec.hdr.player, dec.hdr.kind);
            if j == 0 {
                let key = self
                    .root_moves
                    .iter()
                    .find(|(_, m)| *m == a)
                    .map(|(k, _)| *k)
                    .ok_or_else(|| {
                        EngineError::Assertion("the root move is no root move".into())
                    })?;
                path.push(Step {
                    key: self.child_key(player, kind, key),
                    kind,
                    action: a,
                });
                apply_unchecked(&mut s, a)?;
                continue;
            }
            let moves = canonical(&s, &dec);
            if moves.len() == 1 {
                apply_unchecked(&mut s, a)?;
                continue;
            }
            if player == self.searcher {
                let obs = transpose::key(&s, self.searcher);
                let slot = match self.index.get(&obs) {
                    Some(&i) => i,
                    None => {
                        self.index.insert(obs, self.paths.len());
                        self.paths.push(Vec::new());
                        self.paths.len() - 1
                    }
                };
                // With `transpositions` a node is its observation key (`mcts.rs`, `descend`).
                let node = if self.cfg.mechanisms.transpositions {
                    Vec::new()
                } else {
                    path.clone()
                };
                if !self.paths[slot].contains(&node) {
                    self.paths[slot].push(node);
                }
            }
            let key = moves
                .iter()
                .find(|(_, m)| *m == a)
                .map(|(k, _)| *k)
                .ok_or_else(|| EngineError::Assertion("a walk move is no canonical move".into()))?;
            apply_unchecked(&mut s, a)?;
            if self.cfg.observation_keys
                && player != self.searcher
                && kind == DecisionKind::ActionStep
                && a.kind == ActionKind::Pass
            {
                continue;
            }
            path.push(Step {
                key: self.child_key(player, kind, key),
                kind,
                action: a,
            });
        }
        Ok(())
    }

    fn census(&self, iterations: u32) -> SplitCensus {
        let mut c = SplitCensus {
            iterations,
            ..SplitCensus::default()
        };
        for paths in &self.paths {
            c.observation_keys += 1;
            c.searcher_nodes += u32::try_from(paths.len()).expect("few nodes");
            for p in &paths[1..] {
                c.splits += 1;
                c.count(cause(&paths[0], p, self.searcher));
            }
        }
        c
    }
}

impl SearchTracer for Recorder<'_> {
    fn iteration(&mut self, world: &GameState, it: &IterTrace) {
        let walk: Vec<Action> = std::mem::take(&mut self.actions);
        if self.error.is_some() {
            return;
        }
        let n = (it.applies_walk as usize).min(walk.len());
        if let Err(e) = self.replay(world, &walk[..n]) {
            self.error = Some(e);
        }
    }

    fn apply(&mut self, _n: u32, _j: u32, action: Action, _after: &GameState) {
        self.actions.push(action);
    }
}

/// The census of one search of `cfg` at `player`'s decision in `st` (the decision's generator
/// seeded `seed`); the search's result too.
///
/// # Errors
/// The search's errors, and an engine error of a replayed walk.
pub(crate) fn census(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
    cx: &mut AgentCx<'_>,
) -> Result<(SplitCensus, SearchResult), AgentError> {
    if cfg.mechanisms.macro_multi_pick || cfg.mechanisms.in_tree_filters {
        return Err(AgentError::Value(
            "the split census reconstructs the nodes of canonical single moves: \
             macro_multi_pick and in_tree_filters are not supported"
                .to_owned(),
        ));
    }
    let obs = Observation::new(st, player)?;
    let info = obs.info();
    let dec = info
        .pending()
        .ok_or_else(|| AgentError::Assertion(String::new()))?;
    let mut rec = Recorder {
        searcher: player,
        cfg,
        root_moves: canonical(info, &dec),
        actions: Vec::new(),
        index: LookupMap::new(),
        paths: Vec::new(),
        error: None,
    };
    let mut rng = SplitMix64::new(seed);
    let result = crate::mcts::search(&obs, cfg, &mut rng, cx, &mut rec)?;
    if let Some(e) = rec.error.take() {
        return Err(e.into());
    }
    Ok((rec.census(result.stats.iterations), result))
}
