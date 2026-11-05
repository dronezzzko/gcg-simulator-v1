//! `lethal_root` (docs/port/PLAN.md §12.3 S3; W-17, W-03): the tactical solver at the root, on
//! the first K worlds the search's own iterations use (`p6e::worlds::Worlds`, built through the
//! composed `Mechanisms::world`).
//!
//! - **The searcher's lethal.** During the searcher's turn, when the static attack step (or,
//!   outside the MAIN decision or with a setup play available, its upper bound) leaves lethal
//!   possible, the solver looks for a lethal line in world 0 that starts with one of the root
//!   moves (at a MAIN decision an attack on the player, or a Command, ability, pairing or Unit
//!   play followed by attacks: W-17's deploy, pair and removal orders), and checks its first
//!   move in worlds 1..K. A first move that is lethal in every one of the K worlds is the only
//!   root move left, so the search plays it at once.
//! - **The opponent's lethal.** During the opponent's turn, when its remaining attacks can reach
//!   lethal by the static bound, each root move is played in each world and the solver asks
//!   whether the opponent then wins the turn whatever the searcher does. A move after which it
//!   does in every one of the K worlds is dropped, unless every move is (W-03: the defence the
//!   rollouts would only find by chance).
//!
//! Every solver apply is counted as the decision's root applies, at most [`ROOT_APPLY_CAP`].

use gcg_ai::{MoveKey, apply_unchecked};
use gcg_engine::{Action, ActionKind, DecisionKind, EngineError, GameState};

use super::DecisionState;
use super::board::{Board, Turn};
use super::solver::{Limits, Res, Verdict, first_moves, forced_win, lethal_line};
use crate::mech::p6e::worlds::Worlds;
use crate::mech::{Mechanisms, RootCx};

/// K: the worlds a root lethal must hold in.
pub(crate) const ROOT_WORLDS: u32 = 4;

/// The bound of the searcher's lethal search in one world: one setup play, its own action-step
/// plays branched.
pub(crate) const LETHAL_LIMITS: Limits = Limits {
    max_applies: 3000,
    setups: 1,
    own_tricks: true,
};

/// The bound of the opponent's forced-lethal check after one root move in one world.
pub(crate) const DEFENCE_LIMITS: Limits = Limits {
    max_applies: 1500,
    setups: 0,
    own_tricks: true,
};

/// The most solver applies the root judgements of one decision may spend.
pub(crate) const ROOT_APPLY_CAP: u64 = 24_000;

/// Whether `attacker` may still have lethal this turn by the static board: at its MAIN
/// decision the exact static attack step (with a setup play available, its upper bound
/// ignoring the defender's Blockers plus one more hit), elsewhere (an effect or a battle may
/// still change the Blockers) the hits of the rest of its turn with no Blocker at all.
fn may_be_lethal(
    st: &GameState,
    attacker: u8,
    main: bool,
    setups: bool,
) -> Result<bool, EngineError> {
    if !main {
        let b = Board::of(st, attacker, Turn::Rest)?;
        return Ok(b.hits_without_blockers() >= b.hits_needed());
    }
    let mut b = Board::of(st, attacker, Turn::Now)?;
    if b.may_be_lethal() {
        return Ok(true);
    }
    if !setups {
        return Ok(false);
    }
    b.blockers.clear();
    for a in &mut b.attackers {
        a.blockable_by = 0;
    }
    Ok(b.hits_upper_bound() + 1 >= b.hits_needed())
}

/// The root moves with `lethal_root`: `moves` (the root moves of the other mechanisms) reduced
/// to the searcher's proved lethal move, or without the moves that lose to the opponent's
/// forced lethal.
///
/// # Errors
/// An unimplemented card's error; the engine's errors of the world mechanism and of the static
/// board.
pub(crate) fn root_moves<M: Mechanisms + ?Sized>(
    mech: &M,
    state: &DecisionState,
    cx: &mut RootCx<'_>,
    moves: Vec<(MoveKey, Action)>,
) -> Result<Vec<(MoveKey, Action)>, EngineError> {
    if moves.len() < 2 {
        return Ok(moves);
    }
    let mut worlds = Worlds::new(mech, cx.st, cx.player, cx.cfg, cx.rng, 0, ROOT_WORLDS);
    if cx.st.active() == cx.player {
        if let Some(a) = own_lethal(cx, &moves, &mut worlds, state)? {
            return Ok(moves.into_iter().filter(|&(_, x)| x == a).take(1).collect());
        }
        return Ok(moves);
    }
    avoid_lethal(cx, moves, &mut worlds, state)
}

/// The searcher's first move that is lethal in every one of the K worlds.
fn own_lethal<M: Mechanisms + ?Sized>(
    cx: &RootCx<'_>,
    moves: &[(MoveKey, Action)],
    worlds: &mut Worlds<'_, M>,
    state: &DecisionState,
) -> Result<Option<Action>, EngineError> {
    let (st, dec, me) = (cx.st, cx.dec, cx.player);
    let actions: Vec<Action> = moves.iter().map(|&(_, a)| a).collect();
    let main = dec.hdr.kind == DecisionKind::Main;
    let first = first_moves(dec, &actions, true);
    let setups = first.iter().any(|a| a.kind != ActionKind::Attack);
    if first.is_empty() || !may_be_lethal(st, me, main, main && setups)? {
        return Ok(None);
    }
    let w0 = worlds.get(0)?;
    let out = lethal_line(w0, me, LETHAL_LIMITS, &cx.knobs, &first)?;
    state.spend_root(out.applies);
    let Verdict::Lethal(a) = out.verdict else {
        return Ok(None);
    };
    for k in 1..worlds.len() {
        if state.root_applies() >= ROOT_APPLY_CAP {
            return Ok(None);
        }
        let w = worlds.get(k)?;
        let out = lethal_line(w, me, LETHAL_LIMITS, &cx.knobs, &[a])?;
        state.spend_root(out.applies);
        if out.verdict != Verdict::Lethal(a) {
            return Ok(None);
        }
    }
    Ok(Some(a))
}

/// `moves` without those after which the opponent wins this turn in every one of the K worlds,
/// unless that is every move.
fn avoid_lethal<M: Mechanisms + ?Sized>(
    cx: &RootCx<'_>,
    moves: Vec<(MoveKey, Action)>,
    worlds: &mut Worlds<'_, M>,
    state: &DecisionState,
) -> Result<Vec<(MoveKey, Action)>, EngineError> {
    let opp = 1 - cx.player;
    if !may_be_lethal(cx.st, opp, false, false)? {
        return Ok(moves);
    }
    let mut losing = vec![false; moves.len()];
    for (i, &(_, m)) in moves.iter().enumerate() {
        let mut all = true;
        for k in 0..worlds.len() {
            if state.root_applies() >= ROOT_APPLY_CAP {
                return Ok(moves);
            }
            let mut t = worlds.get(k)?.clone();
            state.spend_root(1);
            match apply_unchecked(&mut t, m) {
                Ok(()) => {}
                Err(e @ EngineError::Unimplemented(_)) => return Err(e),
                Err(_) => {
                    all = false;
                    break;
                }
            }
            let (r, applies) = forced_win(&t, opp, DEFENCE_LIMITS, &cx.knobs)?;
            state.spend_root(applies);
            if r != Res::Win {
                all = false;
                break;
            }
        }
        losing[i] = all;
    }
    if losing.iter().all(|&l| l) {
        return Ok(moves);
    }
    Ok(moves
        .into_iter()
        .zip(losing)
        .filter(|&(_, l)| !l)
        .map(|(m, _)| m)
        .collect())
}
