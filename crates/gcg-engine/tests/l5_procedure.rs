//! Layer 5, procedure pieces that need no rules beyond the state itself: `concede` on setup,
//! mid-game (with frames) and finished states, the AI's trial write
//! `procedures::set_rested_for_trial`, the decision cap of `advance` (strict `>`), and the W-47
//! no-progress guard (Python loops forever there) — against the Python vectors where Python
//! has an answer (`engine/game.py:734-771`, `ai/actions.py:228-229`).

mod l5_common;

use gcg_engine::canon::{from_canonical, state_digest};
use gcg_engine::testkit::engine::advance;
use gcg_engine::{
    Action, ActionKind, EndReason, EngineError, NO_PROGRESS_STEPS, NewGameOptions, Step, apply,
    concede, procedures,
};
use l5_common::{int, load, record, records, reg, snapshot, start_game, u64_of};

#[test]
fn concede_matches_python_on_every_state() {
    let mut with_frames = 0;
    for rec in records("state") {
        let id = rec["id"].as_str().unwrap();
        let st = load(&rec["snapshot"]);
        for p in 0u8..2 {
            let mut c = st.clone();
            concede(&mut c, p);
            let want = rec["concede"][usize::from(p)].as_str().unwrap();
            assert_eq!(state_digest(&c), want, "{id}: player {p} concedes");
            assert!(c.frames().is_empty() && c.pending_triggers().is_empty());
            assert_eq!(c.batches().count(), 0, "{id}");
        }
        if int(&rec["frames"]) > 0 {
            with_frames += 1;
        }
    }
    assert!(with_frames >= 2, "{with_frames} states with frames");
}

#[test]
fn trial_rest_writes_the_card_and_touches_once() {
    let mut n = 0;
    for rec in records("state") {
        let Some(trial) = rec["rested_trial"].as_object() else {
            continue;
        };
        let mut st = load(&rec["snapshot"]);
        let v = st.version();
        let uid = u16::try_from(trial["uid"].as_u64().unwrap()).unwrap();
        procedures::set_rested_for_trial(&mut st, uid, trial["rested"].as_bool().unwrap());
        assert_eq!(st.version(), v + 1);
        assert_eq!(
            state_digest(&st),
            trial["digest"].as_str().unwrap(),
            "{}",
            rec["id"]
        );
        n += 1;
    }
    assert!(n >= 10, "{n}");
}

#[test]
fn the_decision_cap_ends_the_game_after_max_actions_plus_one() {
    let t = record("testkit", "setup");
    let o = NewGameOptions {
        max_actions: 0,
        ..NewGameOptions::default()
    };
    let mut st = start_game(t, o).unwrap();
    assert_eq!(state_digest(&st), t["capped_before"].as_str().unwrap());
    apply(&mut st, Action::new(ActionKind::GoFirst, 1, -1, -1), true).unwrap();
    let c = &t["capped"];
    assert_eq!(state_digest(&st), c["digest"].as_str().unwrap());
    assert_eq!(st.winner(), Some(-1));
    assert_eq!(st.end_reason(), Some(EndReason::TurnLimit));
    assert_eq!((st.step(), st.action_count()), (Step::GameOver, 1));
    assert_eq!(st.rng_state(), u64_of(&c["rng"]));
    assert!(st.pending().is_none());
}

#[test]
fn advance_without_progress_fails_instead_of_looping() {
    // W-47: a setup phase at a step `_setup_step` has no row for never creates a decision;
    // Python's `advance` loops forever, Rust stops after NO_PROGRESS_STEPS iterations.
    let rec = record("state", "broken/base");
    let mut snap = snapshot(&rec["snapshot"]);
    snap.state["step"] = serde_json::json!(Step::Main.value());
    snap.state["pending"] = serde_json::Value::Null;
    let mut st = from_canonical(&reg(), &snap).expect("the snapshot loads");
    let before = state_digest(&st);
    assert_eq!(
        advance(&mut st),
        Err(EngineError::NoProgress {
            steps: NO_PROGRESS_STEPS
        })
    );
    assert_eq!(state_digest(&st), before, "nothing ran");
}
