//! Playing and replaying single games (`runner/game.py`).
//!
//! Agents get the true state read-only (`&GameState`, W-135), so Python's "mutated the game
//! state" check (`runner/game.py:92-93`) cannot fail and has no counterpart (W-49). Between
//! decisions the runner checks the SIGINT flag (PLAN §7.5) and whether a lower match of the
//! run has failed (`TaskCx::superseded`, W-149); Python has neither check, because it
//! terminates its worker processes instead.

use std::fmt;

use gcg_ai::{Agent, AgentCx, AgentError, LogEntry};
use gcg_effects::Registry;
use gcg_engine::{
    Action, ApplyError, EndReason, GameState, NewGameError, NewGameOptions, apply, canon, new_game,
};

use crate::config::{agent_seed, dut_seat, game_seed, seat_decks};
use crate::instrument::{GameObserver, PyFailure};
use crate::{
    AgentFactory, AgentSpec, BenchmarkConfig, Format, GameRecord, INTERRUPTED_MESSAGE, Outcome,
    ReplayError, Role, RunError,
};

/// `str(exc)` of `GameFailure` (`runner/game.py:30-31`), the text after `"error: simulation
/// failed: "` (`cli.py:208-209`); an interrupt prints [`INTERRUPTED_MESSAGE`].
impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GameFailure(message) => f.write_str(message),
            Self::Interrupted => f.write_str(INTERRUPTED_MESSAGE),
        }
    }
}

impl std::error::Error for RunError {}

/// `_role(seat, dut)` (`runner/game.py:34-35`).
const fn role(seat: u8, dut: u8) -> Role {
    if seat == dut { Role::Dut } else { Role::Bench }
}

/// `result_label(winner_seat, dut)` (`runner/game.py:38-39`): `draw` for a negative winner.
#[must_use]
pub const fn result_label(winner_seat: i8, dut: u8) -> Outcome {
    if winner_seat < 0 {
        Outcome::Draw
    } else if winner_seat as u8 == dut {
        Outcome::Dut
    } else {
        Outcome::Bench
    }
}

/// `match_point(st, dut, fmt)` (`runner/game.py:42-51`) on the state's `winner`,
/// `end_reason` and `active`: who scores the game for the match. A simultaneous defeat has no
/// game winner; in BO3 the turn player at the end loses it (TRM 5.2). A turn-limit draw scores
/// for nobody.
///
/// # Errors
/// `GameFailure("the game has not ended")` when there is no winner yet.
pub fn match_point_of(
    winner: Option<i8>,
    end_reason: Option<EndReason>,
    active: u8,
    dut: u8,
    format: Format,
) -> Result<Option<Role>, RunError> {
    let Some(winner) = winner else {
        return Err(RunError::GameFailure("the game has not ended".to_owned()));
    };
    if winner >= 0 {
        #[expect(clippy::cast_sign_loss, reason = "winner >= 0 is a seat")]
        return Ok(Some(role(winner as u8, dut)));
    }
    if format == Format::Bo3 && end_reason == Some(EndReason::BothDefeated) {
        return Ok(Some(role(1 - active, dut)));
    }
    Ok(None)
}

/// `match_point(st, dut, fmt)` (`runner/game.py:42-51`).
///
/// # Errors
/// As [`match_point_of`].
pub fn match_point(st: &GameState, dut: u8, format: Format) -> Result<Option<Role>, RunError> {
    match_point_of(st.winner(), st.end_reason(), st.active(), dut, format)
}

/// `match_loser_seat(record)` (`runner/game.py:54-59`): the seat that chooses Player One next
/// game (the match-scoring loser), if any.
#[must_use]
pub const fn match_loser_seat(record: &GameRecord) -> Option<u8> {
    let Some(point) = record.match_point else {
        return None;
    };
    let winner_seat = match point {
        Role::Dut => record.dut_seat,
        Role::Bench => record.bench_seat(),
    };
    Some(1 - winner_seat)
}

/// `_make_agents(config, m, g, factory)` (`runner/game.py:62-76`): seat 0's agent, then seat
/// 1's, each built from a fresh [`AgentSpec`].
fn make_agents(
    cfg: &BenchmarkConfig,
    m: u32,
    g: u32,
    f: &dyn AgentFactory,
) -> Result<[Box<dyn Agent>; 2], AgentError> {
    let dut = dut_seat(m);
    let spec = |seat: u8| AgentSpec {
        role: role(seat, dut),
        seat,
        preset: cfg.ai_preset,
        seed: agent_seed(cfg.seed, m, g, seat),
        log: cfg.decision_log,
    };
    let first = f.make(&spec(0))?;
    let second = f.make(&spec(1))?;
    Ok([first, second])
}

/// Why `_drive` stopped without finishing the game.
enum Stop {
    /// An exception Python would raise (wrapped into the game's `GameFailure`).
    Failure(PyFailure),
    /// SIGINT (PLAN §7.5).
    Interrupted,
    /// A lower match of the run failed (W-149); the result is never reported.
    Superseded,
}

impl From<PyFailure> for Stop {
    fn from(f: PyFailure) -> Self {
        Self::Failure(f)
    }
}

fn agent_failure(e: AgentError) -> Stop {
    match e {
        AgentError::Interrupted => Stop::Interrupted,
        e => Stop::Failure(PyFailure {
            class: e.py_class(),
            message: e.to_string(),
        }),
    }
}

/// `type(exc).__name__` of what `apply` raises.
const fn apply_class(e: &ApplyError) -> &'static str {
    match e {
        ApplyError::Illegal(_) => "IllegalActionError",
        ApplyError::Engine(e) => e.py_class(),
    }
}

/// `type(exc).__name__` of what `new_game` raises.
const fn new_game_class(e: &NewGameError) -> &'static str {
    match e {
        NewGameError::Construction(_) => "DeckConstructionError",
        NewGameError::Unimplemented(_) => "UnimplementedCardError",
        NewGameError::Engine(e) => e.py_class(),
    }
}

/// `_drive(st, agents, observer, log)` (`runner/game.py:79-103`): ask the deciding seat's
/// agent, append both seats' new log entries in decision order, observe, apply, record.
fn drive(
    st: &mut GameState,
    agents: &mut [Box<dyn Agent>; 2],
    observer: &mut GameObserver,
    mut log: Option<&mut Vec<LogEntry>>,
    cx: &mut AgentCx<'_>,
) -> Result<Vec<Action>, Stop> {
    let mut actions = Vec::new();
    let mut logged = [0_usize; 2];
    loop {
        if cx.cancel.is_cancelled() {
            return Err(Stop::Interrupted);
        }
        if cx.par.superseded() {
            return Err(Stop::Superseded);
        }
        let Some(decision) = st.pending() else {
            return Ok(actions);
        };
        let (kind, player) = (decision.hdr.kind, decision.hdr.player);
        let seat = usize::from(player);
        let agent = &mut agents[seat];
        let action = agent.choose(st, player, cx).map_err(agent_failure)?;
        if let Some(log) = log.as_deref_mut() {
            let entries = agent.decision_log();
            log.extend_from_slice(entries.get(logged[seat]..).unwrap_or_default());
            logged[seat] = entries.len();
        }
        observer.before(st, kind, player, action)?;
        apply(st, action, true).map_err(|e| PyFailure {
            class: apply_class(&e),
            message: e.to_string(),
        })?;
        observer.after(st, kind, player, action);
        actions.push(action);
    }
}

/// `play_game(config, match_index, game_index, chooser, agent_factory=...)`
/// (`runner/game.py:106-166`): play game `g` (0-based) of match `m` to completion. `chooser`
/// is `new_game`'s argument (`None`: the seeded die roll).
///
/// # Errors
/// [`RunError::GameFailure`] with Python's message for anything the game raises (`"match {m}
/// game {g + 1} (seed {seed}) failed: {Class}: {exc}"`, or `"... stopped without a result"`);
/// [`RunError::Interrupted`] on SIGINT.
pub fn play_game(
    cfg: &BenchmarkConfig,
    m: u32,
    g: u32,
    chooser: Option<u8>,
    f: &dyn AgentFactory,
    cx: &mut AgentCx<'_>,
) -> Result<GameRecord, RunError> {
    let dut = dut_seat(m);
    let seed = game_seed(cfg.seed, m, g);
    let failed = |class: &str, message: &dyn fmt::Display| {
        RunError::GameFailure(format!(
            "match {m} game {} (seed {seed}) failed: {class}: {message}",
            g + 1
        ))
    };
    if cx.cancel.is_cancelled() {
        return Err(RunError::Interrupted);
    }
    let decks = seat_decks(cfg, m).map_err(|e| failed("DeckError", &e))?;
    let opts = NewGameOptions {
        chooser,
        turn_limit: cfg.turn_limit,
        max_actions: cfg.max_actions,
        validate: true,
    };
    let mut st = new_game(&Registry::embedded(), [&decks[0], &decks[1]], seed, opts)
        .map_err(|e| failed(new_game_class(&e), &e))?;
    let mut agents = make_agents(cfg, m, g, f).map_err(|e| match agent_failure(e) {
        Stop::Failure(p) => failed(p.class, &p.message),
        _ => RunError::Interrupted,
    })?;
    let mut observer = GameObserver::new(&st, dut);
    let mut log = cfg.decision_log.then(Vec::new);
    let actions =
        drive(&mut st, &mut agents, &mut observer, log.as_mut(), cx).map_err(
            |stop| match stop {
                Stop::Failure(p) => failed(p.class, &p.message),
                Stop::Interrupted => RunError::Interrupted,
                Stop::Superseded => RunError::GameFailure(format!(
                    "match {m} game {} stopped: a lower match failed",
                    g + 1
                )),
            },
        )?;
    let (Some(winner), Some(end_reason)) = (st.winner(), st.end_reason()) else {
        return Err(RunError::GameFailure(format!(
            "match {m} game {} stopped without a result",
            g + 1
        )));
    };
    Ok(GameRecord {
        match_index: m,
        game_index: g,
        seed,
        agent_seeds: [agent_seed(cfg.seed, m, g, 0), agent_seed(cfg.seed, m, g, 1)],
        agents: [agents[0].name().into(), agents[1].name().into()],
        dut_seat: dut,
        chooser,
        chooser_seat: st.setup_chooser(),
        first_player: st.first_player(),
        dut_on_play: st.first_player() == dut,
        winner: result_label(winner, dut),
        end_reason,
        match_point: match_point(&st, dut, cfg.format)?,
        turns: st.turn(),
        active_at_end: st.active(),
        actions,
        redraws: st.redraws(),
        dut_cards: observer.dut_events(&st),
        bench_seen: observer.bench_seen(),
        final_state_sha256: canon::state_digest(&st),
        turn_limit: cfg.turn_limit,
        max_actions: cfg.max_actions,
        decision_log: log,
        effort: [agents[0].effort(), agents[1].effort()],
    })
}

/// `replay(record, config)` (`runner/game.py:185-194`, `replay_game` `:169-182`): re-create
/// the game from its seed and chooser and re-apply the recorded actions; the result has the
/// recorded final state.
///
/// # Errors
/// What Python raises, as the closest [`ReplayError`]: an unimplemented card, a construction
/// problem (`Malformed`), an action that is not legal (`Illegal`), or an engine error.
pub fn replay(record: &GameRecord, cfg: &BenchmarkConfig) -> Result<GameState, ReplayError> {
    let decks = seat_decks(cfg, record.match_index).map_err(|e| ReplayError::Malformed {
        py_class: "DeckError",
        detail: e.to_string(),
    })?;
    let opts = NewGameOptions {
        chooser: record.chooser,
        turn_limit: record.turn_limit,
        max_actions: record.max_actions,
        validate: true,
    };
    let mut st = new_game(
        &Registry::embedded(),
        [&decks[0], &decks[1]],
        record.seed,
        opts,
    )
    .map_err(crate::replay::new_game_replay_error)?;
    for (index, &action) in record.actions.iter().enumerate() {
        apply(&mut st, action, true).map_err(|e| match e {
            ApplyError::Illegal(_) => ReplayError::Illegal { index, action },
            ApplyError::Engine(e) => ReplayError::Engine(e),
        })?;
    }
    Ok(st)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::DutCardEvents;

    #[test]
    fn match_point_rules() {
        // tests/runner/test_match.py:112-121
        let bo3 = Format::Bo3;
        let bo1 = Format::Bo1;
        let battle = Some(EndReason::BattleDamage);
        let both = Some(EndReason::BothDefeated);
        assert_eq!(
            match_point_of(Some(1), battle, 0, 0, bo3),
            Ok(Some(Role::Bench))
        );
        assert_eq!(
            match_point_of(Some(-1), both, 1, 0, bo3),
            Ok(Some(Role::Dut))
        );
        assert_eq!(
            match_point_of(Some(-1), both, 1, 1, bo3),
            Ok(Some(Role::Bench))
        );
        assert_eq!(match_point_of(Some(-1), both, 1, 0, bo1), Ok(None));
        let limit = Some(EndReason::TurnLimit);
        assert_eq!(match_point_of(Some(-1), limit, 1, 0, bo3), Ok(None));
        assert_eq!(
            match_point_of(None, None, 0, 0, bo3),
            Err(RunError::GameFailure("the game has not ended".to_owned()))
        );
    }

    #[test]
    fn result_labels() {
        assert_eq!(result_label(-1, 0), Outcome::Draw);
        assert_eq!(result_label(0, 0), Outcome::Dut);
        assert_eq!(result_label(1, 0), Outcome::Bench);
        assert_eq!(result_label(1, 1), Outcome::Dut);
    }

    pub(crate) fn fake_record(
        m: u32,
        g: u32,
        chooser: Option<u8>,
        winner: Outcome,
        point: Option<Role>,
        reason: EndReason,
    ) -> GameRecord {
        // tests/runner/test_match.py:20-46
        GameRecord {
            match_index: m,
            game_index: g,
            seed: 0,
            agent_seeds: [0, 0],
            agents: ["a".into(), "b".into()],
            dut_seat: dut_seat(m),
            chooser,
            chooser_seat: chooser.unwrap_or(0),
            first_player: 0,
            dut_on_play: m.is_multiple_of(2),
            winner,
            end_reason: reason,
            match_point: point,
            turns: 10,
            active_at_end: 0,
            actions: Vec::new(),
            redraws: [false, false],
            dut_cards: DutCardEvents {
                initial_hand: Vec::new(),
                redrew: false,
                opening_hand: Vec::new(),
                drawn: Vec::new(),
                played: Vec::new(),
                never_played: Vec::new(),
                in_hand_at_end: Vec::new(),
            },
            bench_seen: Vec::new(),
            final_state_sha256: String::new(),
            turn_limit: 200,
            max_actions: 20_000,
            decision_log: None,
            effort: [None, None],
        }
    }

    #[test]
    fn the_match_scoring_loser_chooses_next() {
        let r = |m, point| fake_record(m, 0, None, Outcome::Draw, point, EndReason::TurnLimit);
        assert_eq!(match_loser_seat(&r(0, None)), None);
        // Match 0: the deck under test sits in seat 0.
        assert_eq!(match_loser_seat(&r(0, Some(Role::Dut))), Some(1));
        assert_eq!(match_loser_seat(&r(0, Some(Role::Bench))), Some(0));
        // Match 1: the deck under test sits in seat 1.
        assert_eq!(match_loser_seat(&r(1, Some(Role::Dut))), Some(0));
        assert_eq!(match_loser_seat(&r(1, Some(Role::Bench))), Some(1));
    }

    #[test]
    fn run_errors_print_the_game_failure() {
        let e = RunError::GameFailure("match 0 game 1 (seed 5) failed: X: y".to_owned());
        assert_eq!(e.to_string(), "match 0 game 1 (seed 5) failed: X: y");
        assert_eq!(RunError::Interrupted.to_string(), INTERRUPTED_MESSAGE);
    }
}
