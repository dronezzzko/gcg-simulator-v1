//! Self-play games between agents (`ai/selfplay.py:60-118`): `run_game`, `play` and
//! `play_all`, on the work pool (PLAN §7.1), with results in spec order for any worker count.
//!
//! `--choose-first seat0` (PLAN A24, X13) answers the CHOOSE_FIRST decision with `go_first(0)`
//! (`Action(GO_FIRST, 0)`, `engine/types.py:118`) instead of asking the chooser's agent, so
//! seat 0 is on the play in every game; `agents` is Python's protocol (`engine/game.py:153`).
//! The die roll that picks the chooser happens in `new_game` either way.
//!
//! The Phase 6 tools play through [`run_game_with`]: it asks the agents through
//! `Agent::choose_traced` with [`NoTrace`] (whose action, draws, decision counter and log equal
//! `choose`'s, docs/dev/contracts.md gcg-ai), so each seat's deterministic search effort
//! ([`SeatEffort`]: iterations and engine applies, PLAN §6.6) is summed per game, and an
//! [`Observer`] sees every decision (positions for `tune` and `regret`, the decision traces of
//! W-21). Python's protocol paths (`compare`) keep [`play_all`] and plain `choose`.

use std::fmt;
use std::num::NonZeroUsize;

use gcg_ai::{Agent, AgentCx, AgentError, DecisionTrace, NoTrace, Scratch};
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
    /// An agent's error as a game failure (or the interrupt).
    #[must_use]
    pub fn agent(e: AgentError) -> Self {
        match e {
            AgentError::Interrupted => Self::Interrupted,
            other => Self::Failed {
                class: other.py_class(),
                message: agent_error_text(&other),
            },
        }
    }

    /// An engine error raised by a tool's own computation (features, playouts).
    #[must_use]
    pub fn engine(e: &gcg_engine::EngineError) -> Self {
        Self::Failed {
            class: e.py_class(),
            message: e.to_string(),
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

/// What a game shows its observer. Every method defaults to doing nothing.
pub trait Observer {
    /// Before the decision of `player` at `st` (also for a CHOOSE_FIRST the harness answers).
    ///
    /// # Errors
    /// The observer's own failure (an engine error of a feature computation), which ends the
    /// game.
    fn before(&mut self, st: &GameState, player: u8) -> Result<(), GameError> {
        let _ = (st, player);
        Ok(())
    }

    /// After the agent of `player` chose `action` at `st` (not called for a CHOOSE_FIRST the
    /// harness answers), with the agent's report when the game is traced.
    fn after(&mut self, st: &GameState, player: u8, action: Action, trace: Option<&DecisionTrace>) {
        let _ = (st, player, action, trace);
    }

    /// After `action` was applied, for every decision (a CHOOSE_FIRST the harness answers
    /// included), so the calls give the game's action list in order.
    fn applied(&mut self, action: Action) {
        let _ = action;
    }
}

/// The observer that observes nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct NoObserver;

impl Observer for NoObserver {}

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
    run_game_with(
        agents,
        decks,
        seed,
        choose_first,
        cx,
        false,
        &mut NoObserver,
    )
}

/// [`run_game`] with an observer. With `traced` each agent is asked through
/// `choose_traced(.., &mut NoTrace)`, which chooses exactly what `choose` would, and the
/// observer receives the agent's [`DecisionTrace`].
///
/// # Errors
/// As [`run_game`], or the observer's.
pub fn run_game_with(
    agents: &mut [Box<dyn Agent>; 2],
    decks: [&DeckList; 2],
    seed: u64,
    choose_first: ChooseFirst,
    cx: &mut AgentCx<'_>,
    traced: bool,
    observer: &mut dyn Observer,
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
        observer.before(&st, player)?;
        let action = if choose_first == ChooseFirst::Seat0 && kind == DecisionKind::ChooseFirst {
            GO_FIRST_SEAT0
        } else {
            let agent = &mut agents[usize::from(player)];
            let (action, trace) = if traced {
                agent
                    .choose_traced(&st, player, cx, &mut NoTrace)
                    .map_err(GameError::agent)?
            } else {
                (
                    agent.choose(&st, player, cx).map_err(GameError::agent)?,
                    None,
                )
            };
            observer.after(&st, player, action, trace.as_ref());
            action
        };
        apply(&mut st, action, true)?;
        observer.applied(action);
        decisions += 1;
    }
    Ok((st, decisions))
}

/// The deterministic search effort of one seat (PLAN §6.6; W-21): the decisions its agent
/// made, how many of them ran a search (`DecisionTrace::search`), and those searches'
/// iterations, engine applies by purpose, determinised worlds and capped rollouts. Random and
/// greedy run no search, so they report no searches.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct SeatEffort {
    pub decisions: u64,
    pub searches: u64,
    pub iterations: u64,
    pub applies_tree: u64,
    pub applies_rollout: u64,
    pub applies_aux: u64,
    pub worlds: u64,
    pub rollouts_capped: u64,
}

impl SeatEffort {
    /// Counts one decision with what its agent reported.
    pub fn record(&mut self, trace: Option<&DecisionTrace>) {
        self.decisions += 1;
        if let Some(stats) = trace.and_then(|t| t.search.as_ref()) {
            self.searches += 1;
            self.iterations += u64::from(stats.iterations);
            self.applies_tree += stats.effort.applies_tree;
            self.applies_rollout += stats.effort.applies_rollout;
            self.applies_aux += stats.effort.applies_aux;
            self.worlds += stats.effort.worlds;
            self.rollouts_capped += stats.effort.rollouts_capped;
        }
    }

    /// The sum of two counts.
    #[must_use]
    pub const fn plus(&self, o: &Self) -> Self {
        Self {
            decisions: self.decisions + o.decisions,
            searches: self.searches + o.searches,
            iterations: self.iterations + o.iterations,
            applies_tree: self.applies_tree + o.applies_tree,
            applies_rollout: self.applies_rollout + o.applies_rollout,
            applies_aux: self.applies_aux + o.applies_aux,
            worlds: self.worlds + o.worlds,
            rollouts_capped: self.rollouts_capped + o.rollouts_capped,
        }
    }

    /// Engine applies of every purpose.
    #[must_use]
    pub const fn applies(&self) -> u64 {
        self.applies_tree + self.applies_rollout + self.applies_aux
    }
}

/// The observer that sums each seat's [`SeatEffort`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct EffortObserver {
    pub effort: [SeatEffort; 2],
}

impl Observer for EffortObserver {
    fn after(
        &mut self,
        _st: &GameState,
        player: u8,
        _action: Action,
        trace: Option<&DecisionTrace>,
    ) {
        self.effort[usize::from(player)].record(trace);
    }
}

/// One played game: its outcome and each seat's effort (indexed by seat).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GameResult {
    pub outcome: GameOutcome,
    pub effort: [SeatEffort; 2],
}

fn outcome_of(spec: &MirroredSpec, st: &GameState, decisions: u32) -> GameOutcome {
    GameOutcome {
        index: spec.index,
        challenger: spec.challenger,
        winner: st.winner().unwrap_or(-1),
        turns: st.turn(),
        first_player: st.first_player(),
        decisions,
    }
}

/// The two agents of a spec, seat 0's built first (`ai/selfplay.py:78-85`).
#[must_use]
pub fn spec_agents(spec: &MirroredSpec) -> [Box<dyn Agent>; 2] {
    [
        make_agent(&spec.agents[0], spec.agent_seeds[0], false),
        make_agent(&spec.agents[1], spec.agent_seeds[1], false),
    ]
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
    let mut agents = spec_agents(spec);
    let decks = [&spec.decks[0], &spec.decks[1]];
    let (st, decisions) = run_game(&mut agents, decks, spec.game_seed, choose_first, cx)?;
    Ok(outcome_of(spec, &st, decisions))
}

/// [`play`] with each seat's effort, the agents asked through `choose_traced`, and `observer`
/// called at every decision.
///
/// # Errors
/// As [`run_game_with`].
pub fn play_observed(
    spec: &MirroredSpec,
    choose_first: ChooseFirst,
    cx: &mut AgentCx<'_>,
    observer: &mut dyn Observer,
) -> Result<GameResult, GameError> {
    play_observed_final(spec, choose_first, cx, observer).map(|(result, _)| result)
}

/// [`play_observed`] with the game's final state.
///
/// # Errors
/// As [`run_game_with`].
pub fn play_observed_final(
    spec: &MirroredSpec,
    choose_first: ChooseFirst,
    cx: &mut AgentCx<'_>,
    observer: &mut dyn Observer,
) -> Result<(GameResult, GameState), GameError> {
    struct Both<'o> {
        effort: EffortObserver,
        inner: &'o mut dyn Observer,
    }
    impl Observer for Both<'_> {
        fn before(&mut self, st: &GameState, player: u8) -> Result<(), GameError> {
            self.inner.before(st, player)
        }
        fn after(
            &mut self,
            st: &GameState,
            player: u8,
            action: Action,
            trace: Option<&DecisionTrace>,
        ) {
            self.effort.after(st, player, action, trace);
            self.inner.after(st, player, action, trace);
        }
        fn applied(&mut self, action: Action) {
            self.inner.applied(action);
        }
    }
    let mut agents = spec_agents(spec);
    let decks = [&spec.decks[0], &spec.decks[1]];
    let mut both = Both {
        effort: EffortObserver::default(),
        inner: observer,
    };
    let (st, decisions) = run_game_with(
        &mut agents,
        decks,
        spec.game_seed,
        choose_first,
        cx,
        true,
        &mut both,
    )?;
    let result = GameResult {
        outcome: outcome_of(spec, &st, decisions),
        effort: both.effort.effort,
    };
    Ok((result, st))
}

/// [`play_observed`] without an observer of its own.
///
/// # Errors
/// As [`run_game`].
pub fn play_traced(
    spec: &MirroredSpec,
    choose_first: ChooseFirst,
    cx: &mut AgentCx<'_>,
) -> Result<GameResult, GameError> {
    play_observed(spec, choose_first, cx, &mut NoObserver)
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
    run_indexed(
        specs.len(),
        workers,
        cancel,
        progress,
        |i, cx| play(&specs[i], choose_first, cx),
        |i| (specs[i].index, specs[i].game_seed),
    )
}

/// [`play_all`] with each seat's effort ([`play_traced`]).
///
/// # Errors
/// The lowest failing game, or the interrupt.
pub fn play_all_traced(
    specs: &[MirroredSpec],
    workers: Workers,
    choose_first: ChooseFirst,
    cancel: &Cancel,
    progress: &mut dyn FnMut(usize),
) -> Result<Vec<GameResult>, PlayError> {
    run_indexed(
        specs.len(),
        workers,
        cancel,
        progress,
        |i, cx| play_traced(&specs[i], choose_first, cx),
        |i| (specs[i].index, specs[i].game_seed),
    )
}

/// Runs `task(i, cx)` for `i` in `0..n` on the work pool and returns the results in index
/// order for any worker count. `ident(i)` names task `i`'s game index and seed in a failure.
///
/// # Errors
/// The lowest failing task, or the interrupt.
pub fn run_indexed<T: Send>(
    n: usize,
    workers: Workers,
    cancel: &Cancel,
    progress: &mut dyn FnMut(usize),
    task: impl Fn(usize, &mut AgentCx<'_>) -> Result<T, GameError> + Sync,
    ident: impl Fn(usize) -> (u32, u64),
) -> Result<Vec<T>, PlayError> {
    let pool = Pool::new(threads(workers, n));
    let run = |i: usize, scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel,
        };
        task(i, &mut cx)
    };
    pool.map_indexed(n, cancel, run, progress)
        .map_err(|e| match e {
            PoolError::Cancelled => PlayError::Interrupted,
            PoolError::Task { index, error } => {
                let (game, game_seed) = ident(index);
                match error {
                    GameError::Interrupted => PlayError::Interrupted,
                    GameError::Failed { class, message } => PlayError::Game(GameFailure {
                        index: game,
                        game_seed,
                        class,
                        message,
                    }),
                    GameError::Superseded => PlayError::Game(GameFailure {
                        index: game,
                        game_seed,
                        class: "RuntimeError",
                        message: "stopped after a lower game failed".to_owned(),
                    }),
                }
            }
        })
}
