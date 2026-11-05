//! The tactical solver of P6-B (docs/dev/PLAN.md §12.3 S3; W-03, W-16, W-17): an exact bounded
//! AND-OR search of the rest of one player's turn on a determinised world, played by the engine
//! on clones.
//!
//! The attacking player wins a line when the defender is defeated before the turn ends. At the
//! attacker's MAIN decisions the search tries every attack on the player (one per class of
//! interchangeable attackers, `gcg_ai::merge_equivalent`) and, while the line has setup plays
//! left, every Command, ability, pairing and Unit play (the deploy, pair and removal orders of
//! W-17); ending the main phase never wins. The defender's BLOCK and ACTION_STEP decisions are
//! answered by every option (blocks with the engine's eligible Blockers, so CANT_BLOCK,
//! NO_BLOCKER_VS, <High-Maneuver> and rested Blockers are the engine's, W-32; and the
//! defender's determinised tricks); the attacker's own ACTION_STEP decisions are branched or
//! passed by [`Limits::own_tricks`], and its own SELECT and YES_NO decisions (the targets of its
//! Commands and 【Attack】 abilities) are branched. Every other decision (the defender's
//! targets and Bursts, trigger order) takes the reference policy's best option without a random
//! draw, so the solver is deterministic. Shields, Bursts, the Base and its HP, AP and First Strike come from playing
//! the line.
//!
//! A line that reaches [`Limits::max_applies`] engine applies, or whose apply raises an engine
//! error other than an unimplemented card, is unknown; the solver claims a lethal only when
//! every defender reply it explored loses (a proof within the bound). Every apply is counted.

use gcg_ai::{PolicyKnobs, apply_unchecked, canonical, choose, merge_equivalent};
use gcg_engine::{Action, ActionKind, DecisionKind, DecisionView, EngineError, GameState};

use super::board::player_attacks;

/// The most engine applies one line may make (a guard against loops in card effects).
const MAX_LINE: u32 = 256;

/// The bounds of one solve.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// The most engine applies the whole solve may make.
    pub(crate) max_applies: u64,
    /// The most main-phase plays other than attacks one line may make.
    pub(crate) setups: u32,
    /// Whether the attacker's ACTION_STEP decisions branch over its plays (else it passes).
    pub(crate) own_tricks: bool,
}

/// The answer of a solve.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Verdict {
    /// The attacker defeats the defender this turn whatever the defender does; the first move
    /// of a winning line.
    Lethal(Action),
    /// No explored line wins: every candidate first move has a defender reply that holds.
    NoLethal,
    /// The bound was reached before a proof either way.
    Unknown,
}

/// A verdict and the engine applies it cost.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Outcome {
    pub(crate) verdict: Verdict,
    pub(crate) applies: u64,
}

/// The value of a node of the AND-OR search, for the attacker.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Res {
    Win,
    Fail,
    Unknown,
}

/// A candidate move of a branching node and the setup plays the line has left after it.
type Candidate = (Action, u32);

struct Solver<'k> {
    attacker: u8,
    turn: i32,
    limits: Limits,
    knobs: &'k PolicyKnobs,
    applies: u64,
}

/// Whether an apply error ends the solve (an unimplemented card stays a hard failure,
/// `effects/registry.py:143-147`) rather than only its line.
const fn fatal(e: &EngineError) -> bool {
    matches!(e, EngineError::Unimplemented(_))
}

/// The main-phase plays a line may make besides attacks (W-17: deploy, pair and removal).
const fn is_setup(kind: ActionKind) -> bool {
    matches!(
        kind,
        ActionKind::PlayCommand | ActionKind::Activate | ActionKind::Pair | ActionKind::PlayUnit
    )
}

/// The options of `dec` among `actions`, one per class of interchangeable field cards
/// (`gcg_ai::merge_equivalent` over `canonical`), in the options' order.
fn distinct(
    s: &GameState,
    dec: &DecisionView<'_>,
    keep: impl Fn(&Action) -> bool,
) -> Result<Vec<Action>, EngineError> {
    let moves: Vec<_> = canonical(s, dec)
        .into_iter()
        .filter(|(_, a)| keep(a))
        .collect();
    if moves.len() < 2 {
        return Ok(moves.into_iter().map(|(_, a)| a).collect());
    }
    Ok(merge_equivalent(s, dec, moves)?
        .into_iter()
        .map(|(_, a)| a)
        .collect())
}

impl Solver<'_> {
    fn spent(&self) -> bool {
        self.applies >= self.limits.max_applies
    }

    /// Applies `a` to `s`; `false` when the engine raised a non-fatal error (the line is
    /// unknown).
    fn apply(&mut self, s: &mut GameState, a: Action) -> Result<bool, EngineError> {
        self.applies += 1;
        match apply_unchecked(s, a) {
            Ok(()) => Ok(true),
            Err(e) if fatal(&e) => Err(e),
            Err(_) => Ok(false),
        }
    }

    /// The candidates of a branching decision of `s` (OR for the attacker, AND for the
    /// defender), or `None` when the decision takes one move.
    fn candidates(
        &self,
        s: &GameState,
        dec: &DecisionView<'_>,
        setups: u32,
    ) -> Result<Option<(Vec<Candidate>, bool)>, EngineError> {
        let mine = dec.hdr.player == self.attacker;
        if dec.options.len() < 2 {
            return Ok(None);
        }
        Ok(match dec.hdr.kind {
            DecisionKind::Main if mine => {
                let mut c: Vec<Candidate> = distinct(s, dec, |a| {
                    a.kind == ActionKind::Attack && a.b == gcg_engine::PLAYER_TARGET
                })?
                .into_iter()
                .map(|a| (a, setups))
                .collect();
                if setups > 0 {
                    c.extend(
                        distinct(s, dec, |a| is_setup(a.kind))?
                            .into_iter()
                            .map(|a| (a, setups - 1)),
                    );
                }
                Some((c, true))
            }
            DecisionKind::Block if !mine => {
                // The blocks first: a hold is found before the free pass is tried.
                let mut c: Vec<Candidate> = distinct(s, dec, |a| a.kind == ActionKind::Block)?
                    .into_iter()
                    .map(|a| (a, setups))
                    .collect();
                c.extend(
                    dec.options
                        .iter()
                        .filter(|a| a.kind != ActionKind::Block)
                        .map(|&a| (a, setups)),
                );
                Some((c, false))
            }
            DecisionKind::ActionStep if !mine => {
                let mut c: Vec<Candidate> = distinct(s, dec, |a| a.kind != ActionKind::Pass)?
                    .into_iter()
                    .map(|a| (a, setups))
                    .collect();
                c.push((Action::of(ActionKind::Pass), setups));
                Some((c, false))
            }
            // The attacker's own choices inside its effects (a Command's or an 【Attack】
            // ability's targets, "you may"): every distinct option.
            DecisionKind::Select | DecisionKind::YesNo if mine => Some((
                distinct(s, dec, |_| true)?
                    .into_iter()
                    .map(|a| (a, setups))
                    .collect(),
                true,
            )),
            DecisionKind::ActionStep if self.limits.own_tricks => {
                let mut c: Vec<Candidate> = vec![(Action::of(ActionKind::Pass), setups)];
                c.extend(
                    distinct(s, dec, |a| a.kind != ActionKind::Pass)?
                        .into_iter()
                        .map(|a| (a, setups)),
                );
                Some((c, true))
            }
            _ => None,
        })
    }

    /// The single move of a non-branching decision: the attacker passes its action steps
    /// (without `own_tricks`), every other decision takes the reference policy's best option.
    fn forced(&self, s: &GameState, dec: &DecisionView<'_>) -> Result<Action, EngineError> {
        if dec.hdr.kind == DecisionKind::ActionStep
            && dec.hdr.player == self.attacker
            && dec.options.iter().any(|a| a.kind == ActionKind::Pass)
        {
            return Ok(Action::of(ActionKind::Pass));
        }
        choose(s, None, 0.0, self.knobs)
    }

    /// The value of `s` for the attacker.
    fn node(&mut self, mut s: GameState, setups: u32, mut line: u32) -> Result<Res, EngineError> {
        loop {
            if let Some(w) = s.winner() {
                return Ok(if i32::from(w) == i32::from(self.attacker) {
                    Res::Win
                } else {
                    Res::Fail
                });
            }
            if s.turn() != self.turn {
                return Ok(Res::Fail);
            }
            if self.spent() || line >= MAX_LINE {
                return Ok(Res::Unknown);
            }
            let Some(dec) = s.pending() else {
                return Ok(Res::Fail);
            };
            if dec.hdr.kind == DecisionKind::Main && dec.hdr.player != self.attacker {
                return Ok(Res::Fail);
            }
            match self.candidates(&s, &dec, setups)? {
                Some((cands, or)) => return self.branch(&s, &cands, or, line),
                None => {
                    if dec.hdr.kind == DecisionKind::Main
                        && dec.options.iter().all(|a| a.kind == ActionKind::EndMain)
                    {
                        return Ok(Res::Fail);
                    }
                    let a = match self.forced(&s, &dec) {
                        Ok(a) => a,
                        Err(e) if fatal(&e) => return Err(e),
                        Err(_) => return Ok(Res::Unknown),
                    };
                    if dec.hdr.kind == DecisionKind::Main && a.kind == ActionKind::EndMain {
                        return Ok(Res::Fail);
                    }
                    if !self.apply(&mut s, a)? {
                        return Ok(Res::Unknown);
                    }
                    line += 1;
                }
            }
        }
    }

    /// An OR (`or`, the attacker chooses) or AND (the defender chooses) node over `cands`.
    fn branch(
        &mut self,
        s: &GameState,
        cands: &[Candidate],
        or: bool,
        line: u32,
    ) -> Result<Res, EngineError> {
        let mut unknown = false;
        for &(a, setups) in cands {
            if self.spent() {
                return Ok(Res::Unknown);
            }
            let r = self.child(s, a, setups, line)?;
            match (or, r) {
                (true, Res::Win) => return Ok(Res::Win),
                (false, Res::Fail) => return Ok(Res::Fail),
                (_, Res::Unknown) => unknown = true,
                _ => {}
            }
        }
        Ok(if unknown {
            Res::Unknown
        } else if or {
            Res::Fail
        } else {
            Res::Win
        })
    }

    /// The value of playing `a` at `s`.
    fn child(
        &mut self,
        s: &GameState,
        a: Action,
        setups: u32,
        line: u32,
    ) -> Result<Res, EngineError> {
        let mut t = s.clone();
        if !self.apply(&mut t, a)? {
            return Ok(Res::Unknown);
        }
        self.node(t, setups, line + 1)
    }
}

/// Whether `attacker` (the turn player) wins the rest of this turn from `s` whatever the
/// defender does, within `limits` (the setup plays of `limits.setups` included).
///
/// # Errors
/// An unimplemented card's error; other engine errors only make their line unknown.
pub(crate) fn forced_win(
    s: &GameState,
    attacker: u8,
    limits: Limits,
    knobs: &PolicyKnobs,
) -> Result<(Res, u64), EngineError> {
    let mut solver = Solver {
        attacker,
        turn: s.turn(),
        limits,
        knobs,
        applies: 0,
    };
    let r = solver.node(s.clone(), limits.setups, 0)?;
    Ok((r, solver.applies))
}

/// A lethal line of `attacker` at its pending decision in `s` that starts with one of `first`,
/// tried in order; first with no setup plays, then with up to `limits.setups` while the bound
/// allows.
///
/// # Errors
/// As [`forced_win`].
pub(crate) fn lethal_line(
    s: &GameState,
    attacker: u8,
    limits: Limits,
    knobs: &PolicyKnobs,
    first: &[Action],
) -> Result<Outcome, EngineError> {
    let mut solver = Solver {
        attacker,
        turn: s.turn(),
        limits,
        knobs,
        applies: 0,
    };
    let mut unknown = false;
    for setups in 0..=limits.setups {
        unknown = false;
        for &a in first {
            if solver.spent() {
                unknown = true;
                break;
            }
            let after = if is_setup(a.kind) {
                if setups == 0 {
                    continue;
                }
                setups - 1
            } else {
                setups
            };
            match solver.child(s, a, after, 0)? {
                Res::Win => {
                    return Ok(Outcome {
                        verdict: Verdict::Lethal(a),
                        applies: solver.applies,
                    });
                }
                Res::Unknown => unknown = true,
                Res::Fail => {}
            }
        }
        if solver.spent() {
            break;
        }
    }
    Ok(Outcome {
        verdict: if unknown {
            Verdict::Unknown
        } else {
            Verdict::NoLethal
        },
        applies: solver.applies,
    })
}

/// The first moves [`lethal_line`] tries at a pending decision of `attacker`: at a MAIN
/// decision the attacks on the player (and, with setups, the setup plays) among `moves`, at
/// any other decision every move.
pub(crate) fn first_moves(dec: &DecisionView<'_>, moves: &[Action], setups: bool) -> Vec<Action> {
    if dec.hdr.kind != DecisionKind::Main {
        return moves.to_vec();
    }
    let mut out = player_attacks(moves);
    if setups {
        out.extend(moves.iter().copied().filter(|a| is_setup(a.kind)));
    }
    out
}
