//! What the strengthened search knows at one decision (docs/dev/PLAN.md §12.3, W-135).
//!
//! The agent adapter (`agent.rs`) receives the true `GameState` through the shared `Agent`
//! trait, builds an [`Observation`] from it at once and passes only the observation to the
//! search (lint §9.9 item 5). The observation does not keep the true state: it holds the
//! observer's *information-set state*, a full state in which every card identity hidden from the
//! observer has been replaced by one drawn from the observer's own pools, so the hidden
//! identities, the hidden history entries and the game generator of the true state are not in
//! it at all. The search builds its worlds from that state.
//!
//! The information-set state is `determinize(st, player, OBSERVATION_SEED)`. `determinize`
//! (`engine/observe.py:56-83`) reassigns every card hidden from the observer from pools that it
//! computes from the observer's visible cards and the decklists only, rewrites every history
//! entry to its card's new identity, replaces the game generator and clears the view cache, so
//! the result depends on the information set alone. Determinising it again with any seed gives
//! the world the reference builds from the true state with that seed: the same cards, history,
//! generator and `rules_checked`, with `version` one higher (one more `touch`). The engine reads
//! `version` only through `rules_checked == version` (`core.py:861`, `core.rs:1702`), which is
//! false at the first check of a world either way (`rules_checked <= version` of the state it was
//! copied from), so every decision is the reference's (this module's tests,
//! `tests/null_config.rs`, `tests/null_config_search_set.rs`).
//!
//! An observation never holds the true state, so it cannot give it back, and outside this crate
//! it gives no state at all: its information-set state is a private field with a crate-private
//! accessor, no method names a card's identity, and it converts into no `GameState` reference.
//!
//! ```compile_fail,E0616
//! fn state<'o>(obs: &'o gcg_search::Observation<'_>) -> &'o gcg_engine::GameState {
//!     &obs.info
//! }
//! ```
//!
//! ```compile_fail,E0624
//! fn state<'o>(obs: &'o gcg_search::Observation<'_>) -> &'o gcg_engine::GameState {
//!     obs.info()
//! }
//! ```
//!
//! ```compile_fail,E0599
//! fn identity(obs: &gcg_search::Observation<'_>, uid: gcg_engine::Uid) -> gcg_data::DefId {
//!     obs.card(uid).def_id
//! }
//! ```
//!
//! ```compile_fail,E0277
//! fn state<'o>(obs: &'o gcg_search::Observation<'_>) -> &'o gcg_engine::GameState {
//!     <gcg_search::Observation<'_> as AsRef<gcg_engine::GameState>>::as_ref(obs)
//! }
//! ```
//!
//! An observation borrows the decision's true state for its lifetime, so it cannot be kept past
//! the decision:
//!
//! ```compile_fail,E0515
//! fn keep(player: u8, st: &gcg_engine::GameState) -> gcg_search::Observation<'static> {
//!     let copy = st.clone();
//!     gcg_search::Observation::new(&copy, player).unwrap()
//! }
//! ```

use std::marker::PhantomData;

use gcg_engine::observe::determinize;
use gcg_engine::{EngineError, GameState};

/// The fixed seed of the information-set state. Any fixed seed works: the state is a function
/// of the information set for every seed.
const OBSERVATION_SEED: u64 = 0x6F62_7365_7276_6564;

/// The strengthened search's view of one decision of `player` (PLAN §12.3, W-135): the
/// observer's own cards, the public zones and the public history, materialised as the
/// information-set state. Built only by the agent adapter (lint §9.9 item 5).
pub struct Observation<'a> {
    info: GameState,
    player: u8,
    /// The observation is of the decision at the true state it was built from.
    decision: PhantomData<&'a GameState>,
}

impl<'a> Observation<'a> {
    /// The observation of `player` at the decision of `st`.
    ///
    /// # Errors
    /// `determinize`'s error: a card hidden from `player` whose owner has no unseen card left
    /// in the decklist (`observe.py:104`); a real game never reaches it (card conservation),
    /// and the reference search fails on it at its first world.
    pub fn new(st: &'a GameState, player: u8) -> Result<Self, EngineError> {
        Ok(Self {
            info: determinize(st, player, OBSERVATION_SEED)?,
            player,
            decision: PhantomData,
        })
    }

    /// The observing player.
    #[must_use]
    pub const fn player(&self) -> u8 {
        self.player
    }

    /// The information-set state: no identity hidden from the observer, no hidden history and
    /// not the true game generator. The search reads only this.
    #[must_use]
    pub(crate) const fn info(&self) -> &GameState {
        &self.info
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use gcg_core::pyjson::Json;
    use gcg_effects::Registry;
    use gcg_engine::StateSnapshot;
    use gcg_engine::canon::{canonical_state, from_canonical};
    use gcg_engine::observe::{information_set_key, permute_hidden};
    use serde_json::Value;

    use super::*;

    fn snapshot_state(reg: &Arc<Registry>, snap: &StateSnapshot) -> GameState {
        from_canonical(reg, snap).expect("a state")
    }

    /// The states of the D2 search vectors, the tier-1 `search` traces and the tier-1 `worlds`
    /// traces (determinised worlds played to the end).
    fn states() -> Vec<GameState> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let reg = Registry::embedded();
        let mut out = Vec::new();
        let vectors = root.join("../gcg-ai/tests/vectors/search/decisions.ndjson");
        for line in std::fs::read_to_string(vectors)
            .expect("the vectors")
            .lines()
        {
            let rec: Value = serde_json::from_str(line).expect("a record");
            let snap = &rec["snapshot"];
            out.push(snapshot_state(
                &reg,
                &StateSnapshot {
                    state: snap["state"].clone(),
                    continuous: snap["continuous"].clone(),
                    filter_sets: snap["filter_sets"].clone(),
                    rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
                },
            ));
        }
        for set in ["search", "worlds"] {
            let dir = root.join("../../oracle/corpus/v1/tier1").join(set);
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .expect("a corpus set")
                .map(|e| e.expect("an entry").path())
                .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
                .collect();
            files.sort();
            for path in files {
                for trace in gcg_diff::read_trace(&path).expect("a trace") {
                    for rec in &trace.records {
                        if let gcg_diff::Record::Snapshot(s) = rec {
                            out.push(snapshot_state(&reg, &s.snapshot));
                        }
                    }
                }
            }
        }
        out
    }

    /// The canonical form with `version` replaced by `version`.
    fn canonical_at(st: &GameState, version: u64) -> Json {
        let mut j = canonical_state(st);
        assert!(j.get("version").is_some());
        j.set("version", version);
        j
    }

    const SEEDS: [u64; 3] = [1, 0x9E37_79B9_7F4A_7C15, 20_261_101];

    #[test]
    fn worlds_from_the_observation_are_the_reference_worlds() {
        let states = states();
        assert!(states.len() > 400, "{}", states.len());
        let mut worlds = 0;
        for st in &states {
            for observer in 0..2u8 {
                let obs = Observation::new(st, observer).expect("an observation");
                for seed in SEEDS {
                    let reference = determinize(st, observer, seed).expect("a world");
                    let ours = determinize(obs.info(), observer, seed).expect("a world");
                    assert_eq!(ours.version(), reference.version() + 1);
                    assert_eq!(ours.rules_checked(), reference.rules_checked());
                    let version = i64::try_from(reference.version()).expect("a version");
                    assert!(reference.rules_checked() < version);
                    assert_eq!(
                        canonical_at(&ours, reference.version()),
                        canonical_state(&reference)
                    );
                    worlds += 1;
                }
            }
        }
        assert_eq!(worlds, states.len() * 2 * SEEDS.len());
    }

    #[test]
    fn the_observation_depends_on_the_information_set_alone() {
        let mut changed = 0;
        let mut total = 0;
        for st in states() {
            for observer in 0..2u8 {
                let obs = Observation::new(&st, observer).expect("an observation");
                let key = information_set_key(&st, observer);
                assert_eq!(information_set_key(obs.info(), observer), key);
                let expected = canonical_at(obs.info(), 0);
                for seed in SEEDS {
                    // Other true states with this information set: the hidden identities
                    // resampled (with the history and the game generator), or the hidden
                    // main-deck identities permuted (with the game generator).
                    for other in [
                        determinize(&st, observer, seed).expect("a world"),
                        permute_hidden(&st, observer, seed),
                    ] {
                        assert_eq!(information_set_key(&other, observer), key);
                        changed += usize::from(canonical_at(&other, 0) != canonical_at(&st, 0));
                        total += 1;
                        let alt = Observation::new(&other, observer).expect("an observation");
                        assert_eq!(canonical_at(alt.info(), 0), expected);
                    }
                }
            }
        }
        // The alternative true states really differ from the true state.
        assert!(changed * 10 >= total * 9, "{changed} of {total}");
    }
}
