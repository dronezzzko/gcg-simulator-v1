//! The common-random-number worlds of P6-E's root judgements (docs/dev/PLAN.md §12.3 S6,
//! W-12): the first K worlds the search's own iterations will use, built through the composed
//! `Mechanisms::world` so that a world mechanism (P6-C) shapes them as it shapes the search's.
//!
//! The core draws the world seeds `W[0:B]` from the per-decision generator right after the
//! root moves (`mcts.rs`, `ai/mcts.py:122-125`). The root mechanisms of P6-E draw nothing from
//! it themselves when they judge in these worlds, so the next K draws of a copy of the
//! generator (after the one draw of the reference's no-effect world, when that check still
//! runs) are exactly `W[0:K]`: no extra determinisation is introduced, and world k of a root
//! judgement is world k of the search.

use gcg_core::rng::SplitMix64;
use gcg_engine::{EngineError, GameState};

use crate::StrengthenedConfig;
use crate::mech::{Mechanisms, WorldCx};

/// The worlds a root judgement uses when the configuration names no count (`prune_worlds`
/// off): `rest_prune_worlds` and `target_rollouts` (W-12's K = 4).
pub(crate) const DEFAULT_WORLDS: u32 = 4;

/// The first K search worlds of one decision, built on first use.
pub(crate) struct Worlds<'a, M: Mechanisms + ?Sized> {
    mech: &'a M,
    st: &'a GameState,
    player: u8,
    cfg: &'a StrengthenedConfig,
    seeds: Vec<u64>,
    built: Vec<Option<GameState>>,
}

impl<'a, M: Mechanisms + ?Sized> Worlds<'a, M> {
    /// The first `k` worlds of the search of `player` at `st`: the seeds are the draws of a copy
    /// of `rng` after `skip` draws (the draws the root makes before the core's `W[0:B]`).
    pub(crate) fn new(
        mech: &'a M,
        st: &'a GameState,
        player: u8,
        cfg: &'a StrengthenedConfig,
        rng: &SplitMix64,
        skip: usize,
        k: u32,
    ) -> Self {
        let mut peek = SplitMix64::new(rng.state);
        for _ in 0..skip {
            peek.next_u64();
        }
        let seeds: Vec<u64> = (0..k).map(|_| peek.next_u64()).collect();
        let built = (0..seeds.len()).map(|_| None).collect();
        Self {
            mech,
            st,
            player,
            cfg,
            seeds,
            built,
        }
    }

    /// The number of worlds.
    pub(crate) fn len(&self) -> usize {
        self.seeds.len()
    }

    /// World `k`, determinised on first use.
    ///
    /// # Errors
    /// The engine's error from the world mechanism.
    pub(crate) fn get(&mut self, k: usize) -> Result<&GameState, EngineError> {
        if self.built[k].is_none() {
            let mut out = self.st.clone();
            let mut cx = WorldCx {
                st: self.st,
                player: self.player,
                cfg: self.cfg,
                seeds: &self.seeds,
            };
            self.mech.world(&mut cx, k, &mut out)?;
            self.built[k] = Some(out);
        }
        Ok(self.built[k].as_ref().expect("built above"))
    }

    /// Every world, each determinised on first use.
    ///
    /// # Errors
    /// The engine's error from the world mechanism.
    pub(crate) fn all(&mut self) -> Result<Vec<&GameState>, EngineError> {
        for k in 0..self.len() {
            self.get(k)?;
        }
        Ok(self
            .built
            .iter()
            .map(|w| w.as_ref().expect("built above"))
            .collect())
    }
}
