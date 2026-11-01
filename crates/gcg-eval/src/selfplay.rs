//! Self-play games between agents (`ai/selfplay.py:60-118`): `run_game`, `play` and
//! `play_all`, on the work pool (PLAN §7.1), with results in spec order for any worker count.
//!
//! `--choose-first seat0` (PLAN A24, X13) answers the CHOOSE_FIRST decision with `go_first(0)`
//! (`Action(GO_FIRST, 0)`, `engine/types.py:118`) instead of asking the chooser's agent, so
//! seat 0 is on the play in every game; `agents` is Python's protocol (`engine/game.py:153`).
//! The die roll that picks the chooser happens in `new_game` either way.

use std::fmt;
use std::num::NonZeroUsize;

use gcg_ai::{Agent, AgentCx, AgentError, Scratch};
use gcg_core::pool::{Cancel, Pool, PoolError, TaskCx};
use gcg_data::DeckList;
use gcg_effects::Registry;
use gcg_engine::{
    Action, ActionKind, ApplyError, DecisionKind, GameState, NO_ARG, NewGameError, NewGameOptions,
    apply, new_game,
};
use gcg_runner::Workers;

use crate::agent_spec::make_agent;
use crate::{ChooseFirst, DECISION_LIMIT, GameOutcome, MirroredSpec};

/// `Action(GO_FIRST, 0)`: seat 0 becomes Player One.
pub const GO_FIRST_SEAT0: Action = Action::new(ActionKind::GoFirst, 0, NO_ARG, NO_ARG);

/// Why a game stopped without an outcome. `class` is the Python exception class.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GameError {
    Failed {
        class: &'static str,
        message: String,
    },
    /// SIGINT (PLAN §7.5).
    Interrupted,
    /// A lower game index already failed (W-149); this result is never reported.
    Superseded,
}

impl GameError {
    fn agent(e: AgentError) -> Self {
        match e {
            AgentError::Interrupted => Self::Interrupted,
            other => Self::Failed {
                class: other.py_class(),
                message: agent_error_text(&other),
            },
        }
    }
}

impl From<NewGameError> for GameError {
    fn from(e: NewGameError) -> Self {
        let class = match &e {
            NewGameError::Construction(_) => "DeckConstructionError",
            NewGameError::Unimplemented(_) => "UnimplementedCardError",
            NewGameError::Engine(inner) => inner.py_class(),
        };
        Self::Failed {
            class,
            message: e.to_string(),
        }
    }
}

impl From<ApplyError> for GameError {
    fn from(e: ApplyError) -> Self {
        let class = match &e {
            ApplyError::Illegal(_) => "IllegalActionError",
            ApplyError::Engine(inner) => inner.py_class(),
        };
        Self::Failed {
            class,
            message: e.to_string(),
        }
    }
}

/// The message of an [`AgentError`] as Python's exception prints it (`str(exc)`).
#[must_use]
pub fn agent_error_text(e: &AgentError) -> String {
    match e {
        AgentError::NoDecision { agent, player } => {
            format!("{agent}: no decision pending for player {player}")
        }
        AgentError::Value(m) | AgentError::Assertion(m) => m.clone(),
        AgentError::Engine(inner) => inner.to_string(),
        AgentError::Illegal(inner) => inner.to_string(),
        AgentError::Interrupted => "interrupted".to_owned(),
    }
}

/// A failed game of a run: its index and seed, and why.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GameFailure {
    pub index: u32,
    pub game_seed: u64,
    pub class: &'static str,
    pub message: String,
}

impl fmt::Display for GameFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "game {} (seed {}) failed: {}: {}",
            self.index, self.game_seed, self.class, self.message
        )
    }
}

/// Why `play_all` returned no outcomes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PlayError {
    /// The lowest failing game (W-149).
    Game(GameFailure),
    Interrupted,
}

/// `run_game(agents, decks, seed)` (`ai/selfplay.py:60-75`): `new_game(decks, seed)` with
/// Python's defaults, then one decision at a time while one is pending and fewer than
/// `DECISION_LIMIT` were made; returns the final state and the decision count.
///
/// # Errors
/// What `new_game`, an agent or `apply` raises; an interrupt or a superseded task between
/// decisions.
pub fn run_game(
    agents: &mut [Box<dyn Agent>; 2],
    decks: [&DeckList; 2],
    seed: u64,
    choose_first: ChooseFirst,
    cx: &mut AgentCx<'_>,
) -> Result<(GameState, u32), GameError> {
    let reg = Registry::embedded();
    let mut st = new_game(&reg, decks, seed, NewGameOptions::default())?;
    let mut decisions = 0;
    while decisions < DECISION_LIMIT {
        if cx.cancel.is_cancelled() {
            return Err(GameError::Interrupted);
        }
        if cx.par.superseded() {
            return Err(GameError::Superseded);
        }
        let Some((player, kind)) = st.pending().map(|d| (d.hdr.player, d.hdr.kind)) else {
            break;
        };
        let action = if choose_first == ChooseFirst::Seat0 && kind == DecisionKind::ChooseFirst {
            GO_FIRST_SEAT0
        } else {
            agents[usize::from(player)]
                .choose(&st, player, cx)
                .map_err(GameError::agent)?
        };
        apply(&mut st, action, true)?;
        decisions += 1;
    }
    Ok((st, decisions))
}

/// `play(spec)` (`ai/selfplay.py:78-85`): builds seat 0's agent, then seat 1's, plays the
/// game, and reports `winner = -1` for a draw or a game the decision limit stopped.
///
/// # Errors
/// As [`run_game`].
pub fn play(
    spec: &MirroredSpec,
    choose_first: ChooseFirst,
    cx: &mut AgentCx<'_>,
) -> Result<GameOutcome, GameError> {
    let mut agents = [
        make_agent(&spec.agents[0], spec.agent_seeds[0], false),
        make_agent(&spec.agents[1], spec.agent_seeds[1], false),
    ];
    let decks = [&spec.decks[0], &spec.decks[1]];
    let (st, decisions) = run_game(&mut agents, decks, spec.game_seed, choose_first, cx)?;
    Ok(GameOutcome {
        index: spec.index,
        challenger: spec.challenger,
        winner: st.winner().unwrap_or(-1),
        turns: st.turn(),
        first_player: st.first_player(),
        decisions,
    })
}

/// The worker threads of a run of `n` games: `--workers N` capped by `n` (`workers <= 1` is
/// the serial path of `play_all`), or `--workers auto` (PLAN §7.4).
#[must_use]
pub fn threads(workers: Workers, n: usize) -> NonZeroUsize {
    let wanted = match workers {
        Workers::Count(w) => usize::try_from(w).unwrap_or(usize::MAX).min(n),
        Workers::Auto => gcg_core::sys::topology::detect().auto_workers(n),
    };
    NonZeroUsize::new(wanted).unwrap_or(NonZeroUsize::MIN)
}

/// `play_all(specs, workers)` (`ai/selfplay.py:113-118`): every game, outcomes in spec order
/// whatever the worker count. `progress(done)` runs on the calling thread after each game.
///
/// # Errors
/// The lowest failing game, or the interrupt.
pub fn play_all(
    specs: &[MirroredSpec],
    workers: Workers,
    choose_first: ChooseFirst,
    cancel: &Cancel,
    progress: &mut dyn FnMut(usize),
) -> Result<Vec<GameOutcome>, PlayError> {
    let pool = Pool::new(threads(workers, specs.len()));
    let task = |i: usize, scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel,
        };
        play(&specs[i], choose_first, &mut cx)
    };
    pool.map_indexed(specs.len(), cancel, task, progress)
        .map_err(|e| match e {
            PoolError::Cancelled => PlayError::Interrupted,
            PoolError::Task { index, error } => match error {
                GameError::Interrupted => PlayError::Interrupted,
                GameError::Failed { class, message } => PlayError::Game(GameFailure {
                    index: specs[index].index,
                    game_seed: specs[index].game_seed,
                    class,
                    message,
                }),
                GameError::Superseded => PlayError::Game(GameFailure {
                    index: specs[index].index,
                    game_seed: specs[index].game_seed,
                    class: "RuntimeError",
                    message: "stopped after a lower game failed".to_owned(),
                }),
            },
        })
}
