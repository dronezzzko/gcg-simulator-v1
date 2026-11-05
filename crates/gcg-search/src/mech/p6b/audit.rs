//! W-17 step (4), feature `slow`: missed lethals in regenerated audit games.
//!
//! The 2026-10 audit's missed-lethal positions were lost with `build/aiq2/audit/` (W-127). This
//! test regenerates 15 BO1 games of the reference `standard` agent on both sides, five per
//! example pairing with the audit's master seeds: 90031 (red-green-zeon vs
//! blue-white-federation), 90032 (blue-white-federation vs purple-white-tekkadan) and 90033
//! (purple-white-tekkadan vs red-green-zeon). Game `g` of master `m` is
//! `new_game(decks, derive_seed(m, "game", g))` with the agents seeded
//! `derive_seed(m, "agent", g, p)`; the agents answer CHOOSE_FIRST and REDRAW. The command and
//! the seed-to-pairing mapping of the lost audit are unknown, so these are reconstructions.
//!
//! At every MAIN decision of the turn player the solver of `lethal_root` runs from the
//! decider's information: the information-set state's first K worlds (`determinize` with
//! `derive_seed(m, "audit", g, n, k)`), a lethal line in world 0 whose first move is lethal in
//! every one of them. Such a decision is a *missed lethal* when the reference's line does not
//! win that turn; it is a *real* one when the solver also proves the lethal on the true state
//! (the analysis may read it; the agents never do). Every real missed lethal is replayed from
//! its state with the strengthened `standard` (reference `standard` budget plus `lethal_root`)
//! against the reference `standard` defender, and must win that turn on all 16 agent seeds.

use std::path::PathBuf;

use gcg_ai::{Agent, AgentCx, Scratch};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::{SeedPart, derive_seed};
use gcg_effects::Registry;
use gcg_engine::observe::determinize;
use gcg_engine::{Action, DecisionKind, GameState, NewGameOptions, apply, new_game_by_number};

use super::root::{LETHAL_LIMITS, ROOT_WORLDS};
use super::solver::{Verdict, first_moves, lethal_line};
use crate::{MechanismFlags, Observation, StrengthenedConfig};

const PAIRINGS: [(u64, &str, &str); 3] = [
    (90031, "red-green-zeon", "blue-white-federation"),
    (90032, "blue-white-federation", "purple-white-tekkadan"),
    (90033, "purple-white-tekkadan", "red-green-zeon"),
];
const GAMES_PER_PAIRING: i64 = 5;
const SEEDS: u64 = 16;

fn with_cx<T>(f: impl FnOnce(&mut AgentCx<'_>) -> T) -> T {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        f(&mut cx)
    })
}

fn deck(name: &str) -> (Vec<String>, Vec<String>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/decks")
        .join(format!("{name}.txt"));
    let d = gcg_data::load_deck(&path).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    let expand = |cards: &[gcg_data::CardCount]| -> Vec<String> {
        cards
            .iter()
            .flat_map(|c| {
                let n = usize::try_from(c.count).expect("a deck count");
                std::iter::repeat_n(c.card_number.to_string(), n)
            })
            .collect()
    };
    (expand(&d.main), expand(&d.resources))
}

fn reference(seed: u64) -> Box<dyn Agent> {
    gcg_ai_reference::make_agent("mcts", "standard", seed, false, None, None).expect("mcts")
}

fn strengthened() -> StrengthenedConfig {
    let mut cfg = super::tests::null_standard();
    cfg.mechanisms = MechanismFlags::NULL;
    cfg.mechanisms.lethal_root = true;
    cfg
}

/// The first move of a lethal line in `worlds[0]` that is lethal in every world.
fn lethal_in_all(
    worlds: &[GameState],
    p: u8,
    first: &[Action],
    cfg: &StrengthenedConfig,
) -> Option<Action> {
    let knobs = cfg.knobs();
    let out = lethal_line(&worlds[0], p, LETHAL_LIMITS, &knobs, first).expect("solve");
    let Verdict::Lethal(a) = out.verdict else {
        return None;
    };
    worlds[1..]
        .iter()
        .all(|w| {
            lethal_line(w, p, LETHAL_LIMITS, &knobs, &[a])
                .expect("solve")
                .verdict
                == Verdict::Lethal(a)
        })
        .then_some(a)
}

struct Flag {
    state: GameState,
    player: u8,
    turn: i32,
    game: String,
    real: bool,
}

fn play(
    master: u64,
    g: i64,
    a: &str,
    b: &str,
    cfg: &StrengthenedConfig,
) -> (Vec<Flag>, Option<i8>, i32) {
    let (da, db) = (deck(a), deck(b));
    let ra: (Vec<&str>, Vec<&str>) = (
        da.0.iter().map(String::as_str).collect(),
        da.1.iter().map(String::as_str).collect(),
    );
    let rb: (Vec<&str>, Vec<&str>) = (
        db.0.iter().map(String::as_str).collect(),
        db.1.iter().map(String::as_str).collect(),
    );
    let seed = derive_seed(master, &[SeedPart::Str("game"), SeedPart::Int(g)]);
    let mut st = new_game_by_number(
        &Registry::embedded(),
        [(&ra.0[..], &ra.1[..]), (&rb.0[..], &rb.1[..])],
        seed,
        NewGameOptions::default(),
    )
    .expect("the example decks are legal");
    let agent_seed = |p: i64| {
        derive_seed(
            master,
            &[SeedPart::Str("agent"), SeedPart::Int(g), SeedPart::Int(p)],
        )
    };
    let mut players = [reference(agent_seed(0)), reference(agent_seed(1))];
    let mut flags = Vec::new();
    let mut n: i64 = 0;
    while let Some(dec) = st.pending() {
        let p = dec.hdr.player;
        if dec.hdr.kind == DecisionKind::Main && p == st.active() && dec.options.len() > 1 {
            n += 1;
            let obs = Observation::new(&st, p).expect("an observation");
            let info = obs.info();
            let worlds: Vec<GameState> = (0..i64::from(ROOT_WORLDS))
                .map(|k| {
                    let s = derive_seed(
                        master,
                        &[
                            SeedPart::Str("audit"),
                            SeedPart::Int(g),
                            SeedPart::Int(n),
                            SeedPart::Int(k),
                        ],
                    );
                    determinize(info, p, s).expect("a world")
                })
                .collect();
            let first = first_moves(&dec, dec.options, true);
            if lethal_in_all(&worlds, p, &first, cfg).is_some() {
                let real = lethal_line(&st, p, LETHAL_LIMITS, &cfg.knobs(), &first)
                    .expect("solve")
                    .verdict;
                flags.push(Flag {
                    state: st.clone(),
                    player: p,
                    turn: st.turn(),
                    game: format!("{master}:{g} {a} vs {b}"),
                    real: matches!(real, Verdict::Lethal(_)),
                });
            }
        }
        let action = with_cx(|cx| players[usize::from(p)].choose(&st.clone(), p, cx))
            .unwrap_or_else(|e| panic!("choose: {e}"));
        apply(&mut st, action, true).unwrap_or_else(|e| panic!("apply {action:?}: {e}"));
    }
    (flags, st.winner(), st.turn())
}

/// Plays the rest of `st`'s turn with `players`; the winner, if the game ended.
fn play_turn(st: &mut GameState, players: &mut [Box<dyn Agent>; 2]) -> Option<i8> {
    let turn = st.turn();
    while st.winner().is_none() && st.turn() == turn {
        let Some(dec) = st.pending() else { break };
        let p = dec.hdr.player;
        let a = with_cx(|cx| players[usize::from(p)].choose(&st.clone(), p, cx))
            .unwrap_or_else(|e| panic!("choose: {e}"));
        apply(st, a, true).unwrap_or_else(|e| panic!("apply {a:?}: {e}"));
    }
    st.winner()
}

/// Wins of `p` over the agent seeds, `me(seed)` deciding for `p` and the reference for the
/// other player.
fn wins(st: &GameState, p: u8, mut me: impl FnMut(u64) -> Box<dyn Agent>) -> u64 {
    (1..=SEEDS)
        .filter(|&seed| {
            let mine = me(seed);
            let other = reference(seed + 100);
            let mut players = if p == 0 { [mine, other] } else { [other, mine] };
            let mut s = st.clone();
            play_turn(&mut s, &mut players) == Some(i8::try_from(p).expect("a seat"))
        })
        .count() as u64
}

#[test]
fn every_real_missed_lethal_of_the_regenerated_audit_games_is_found() {
    let cfg = strengthened();
    let (mut claimed, mut missed) = (0, Vec::<Flag>::new());
    for (master, a, b) in PAIRINGS {
        for g in 0..GAMES_PER_PAIRING {
            let (flags, winner, last) = play(master, g, a, b, &cfg);
            claimed += flags.len();
            for f in flags {
                let won_then =
                    winner == Some(i8::try_from(f.player).expect("a seat")) && last == f.turn;
                let seen = missed.iter().any(|m| m.game == f.game && m.turn == f.turn);
                if !won_then && !seen {
                    missed.push(f);
                }
            }
        }
    }
    let mut real = Vec::new();
    for f in &missed {
        let reference_wins = wins(&f.state, f.player, reference);
        let strengthened_wins = wins(&f.state, f.player, |seed| {
            crate::make_agent(&cfg, seed, false)
        });
        eprintln!(
            "W-17 missed lethal in {} (turn {}, player {}, lethal on the true state: {}): \
             reference standard {reference_wins}/{SEEDS}, lethal_root {strengthened_wins}/{SEEDS}",
            f.game, f.turn, f.player, f.real
        );
        if f.real {
            real.push(strengthened_wins);
        }
    }
    eprintln!(
        "W-17 regenerated audit: {claimed} turn-player MAIN decisions with a lethal proved in all \
         K worlds; {} missed turns by the reference's line, {} of them lethal on the true state \
         (the audit reported two missed lethals)",
        missed.len(),
        real.len()
    );
    // Recorded 2026-11-02: 31 claimed decisions, 1 missed turn (90032:0, turn 15, player 1:
    // not lethal on the true state, the reference and lethal_root both win it 2/16), 0 real
    // missed lethals.
    assert!(
        claimed > 0,
        "the solver proves the lethals the reference plays"
    );
    assert!(
        real.iter().all(|&w| w == SEEDS),
        "every real missed lethal is found on every seed: {real:?}"
    );
}
