//! Per-worker recycled states of the searches (PLAN §4.5, §7.1): every iteration plays in a
//! copy of a determinised world, built in a recycled buffer with `GameState::clone_from_state`
//! so that a warm search reuses its allocations. Recycling never changes a result: a recycled
//! copy is observably equal to `src.clone()` (engine L1).

use gcg_engine::GameState;

use crate::Scratch;

/// Recycled states kept per worker; a search holds one world copy at a time.
const MAX_STATES: usize = 16;

impl Scratch {
    /// A state observably equal to `src.clone()` (`GameState.clone()`,
    /// `engine/state.py:534-572`), built in a recycled buffer when one is free.
    pub fn clone_state(&mut self, src: &GameState) -> GameState {
        match self.states.pop() {
            Some(mut st) => {
                st.clone_from_state(src);
                st
            }
            None => src.clone(),
        }
    }

    /// Returns a state's buffers for reuse.
    pub fn recycle_state(&mut self, st: GameState) {
        if self.states.len() < MAX_STATES {
            self.states.push(st);
        }
    }
}
