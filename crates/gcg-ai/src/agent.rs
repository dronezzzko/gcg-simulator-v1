//! Agent bookkeeping: per-decision seeding, the decision counter and the log hand-off
//! (`ai/base.py:43-76`), the `uid_argument` test (`ai/base.py:125-131`) and Python's messages
//! for [`AgentError`].

use std::fmt;

use gcg_core::rng::{SeedPart, SplitMix64, derive_seed};
use gcg_engine::{Action, CtxKey, DecisionKind, DecisionView, GameState};

use crate::log::log_entry;
use crate::{AgentCore, AgentError, Alternative};

impl AgentCore {
    /// `AgentCore._begin(st, player)` (`ai/base.py:60-65`): the pending decision of `player`
    /// and its generator `SplitMix64(derive_seed(seed, name, "decision", k))`, where `k` is
    /// the decision counter after the increment. The counter goes up for every decision,
    /// single options included (Inv 07 §1.2).
    ///
    /// # Errors
    /// [`AgentError::NoDecision`] when no decision is pending for `player` (the counter does
    /// not move, as in Python).
    pub fn begin<'s>(
        &mut self,
        st: &'s GameState,
        player: u8,
    ) -> Result<(DecisionView<'s>, SplitMix64), AgentError> {
        let dec = match st.pending() {
            Some(dec) if dec.hdr.player == player => dec,
            _ => {
                return Err(AgentError::NoDecision {
                    agent: self.name.clone(),
                    player,
                });
            }
        };
        self.decisions += 1;
        let k = i64::try_from(self.decisions).expect("decision counter fits in i64");
        let seed = derive_seed(
            self.seed,
            &[
                SeedPart::Str(&self.name),
                SeedPart::Str("decision"),
                SeedPart::Int(k),
            ],
        );
        Ok((dec, SplitMix64::new(seed)))
    }

    /// `AgentCore._finish(st, player, chosen, alternatives)` (`ai/base.py:67-76`): appends the
    /// log entry (built from `st` before the action is applied) when logging is on and returns
    /// `chosen`.
    pub fn finish(
        &mut self,
        st: &GameState,
        player: u8,
        chosen: Action,
        alts: &[Alternative],
    ) -> Action {
        if self.log_enabled {
            self.entries.push(log_entry(st, player, chosen, alts));
        }
        chosen
    }
}

/// `uid_argument(dec)` (`ai/base.py:125-131`): whether the SELECT options of `dec` name cards
/// (uids) rather than option indices.
#[must_use]
pub fn uid_argument(dec: &DecisionView<'_>) -> bool {
    match dec.hdr.kind {
        DecisionKind::Excess | DecisionKind::Discard => true,
        DecisionKind::Arrange => dec.hdr.ctx(CtxKey::Order) == 1,
        DecisionKind::Select => dec.hdr.ctx(CtxKey::Mode) != 1,
        _ => false,
    }
}

/// Python's `str(exc)` of the exception each variant stands for: the `ValueError` of
/// `ai/base.py:63`, the agent's own messages, the engine's, and the empty text of
/// `KeyboardInterrupt`.
impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDecision { agent, player } => {
                write!(f, "{agent}: no decision pending for player {player}")
            }
            Self::Value(message) | Self::Assertion(message) => f.write_str(message),
            Self::Engine(e) => fmt::Display::fmt(e, f),
            Self::Illegal(e) => fmt::Display::fmt(e, f),
            Self::Interrupted => Ok(()),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<gcg_engine::EngineError> for AgentError {
    fn from(e: gcg_engine::EngineError) -> Self {
        Self::Engine(e)
    }
}

impl From<gcg_engine::ApplyError> for AgentError {
    fn from(e: gcg_engine::ApplyError) -> Self {
        match e {
            gcg_engine::ApplyError::Illegal(e) => Self::Illegal(e),
            gcg_engine::ApplyError::Engine(e) => Self::Engine(e),
        }
    }
}

/// `apply(st, action, check=False)` for the AI's own clones and worlds, which always have a
/// pending decision at the call sites (`ai/mcts.py:141`, `:226`, `:229`; `ai/noop.py:70-124`;
/// `ai/playout.py:24`), so the only error left is the engine's. An `IllegalActionError`
/// ("no decision pending", `engine/game.py:783-784`) would be an AI bug; it is reported as an
/// internal engine error with Python's message.
///
/// # Errors
/// The engine's error.
pub fn apply_unchecked(st: &mut GameState, action: Action) -> Result<(), gcg_engine::EngineError> {
    gcg_engine::apply(st, action, false).map_err(|e| match e {
        gcg_engine::ApplyError::Engine(e) => e,
        gcg_engine::ApplyError::Illegal(e) => gcg_engine::EngineError::Internal(e.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gcg_engine::EngineError;

    #[test]
    fn errors_print_pythons_messages() {
        let e = AgentError::NoDecision {
            agent: "mcts-standard".into(),
            player: 1,
        };
        assert_eq!(
            e.to_string(),
            "mcts-standard: no decision pending for player 1"
        );
        assert_eq!(AgentError::Interrupted.to_string(), "");
        assert_eq!(
            AgentError::Value("unknown AI preset 'x'".into()).to_string(),
            "unknown AI preset 'x'"
        );
        let engine = EngineError::Value("cannot determinize: empty pool for hidden cards".into());
        assert_eq!(
            AgentError::from(engine.clone()).to_string(),
            engine.to_string()
        );
    }

    /// Inv 07 §1.2 vectors: `derive_seed(17449794008030232776, 'mcts-standard', 'decision', k)`.
    #[test]
    fn per_decision_seeds_follow_the_counter() {
        let core = AgentCore::new("mcts-standard", 17_449_794_008_030_232_776, false);
        let seed = |core: &AgentCore, k: i64| {
            derive_seed(
                core.seed,
                &[
                    SeedPart::Str(&core.name),
                    SeedPart::Str("decision"),
                    SeedPart::Int(k),
                ],
            )
        };
        assert_eq!(seed(&core, 1), 17_683_307_912_657_752_582);
        assert_eq!(seed(&core, 2), 18_101_817_166_437_973_065);
        assert_eq!(seed(&core, 3), 374_240_451_198_152_903);
        let other = AgentCore::new("mcts-standard", 17, false);
        assert_eq!(seed(&other, 1), 3_302_900_377_033_519_914);
        let mut rng = SplitMix64::new(17_683_307_912_657_752_582);
        assert_eq!(
            [rng.next_u64(), rng.next_u64(), rng.next_u64()],
            [
                7_365_148_325_066_467_191,
                16_197_301_673_173_457_939,
                15_328_836_568_995_161_108
            ]
        );
    }
}
