//! D2's root-move functions and the play/draw choice against the Python reference, on the
//! states of `tests/vectors/search/decisions.ndjson` (`port/oracle/export_ai_vectors.py`):
//! `prune_dominated` with all four flag sets, `merge_equivalent`, `without_effect` with the
//! baseline flags and knobs and with every flag on and the adopted knobs, and
//! `first_player_choice` serially and as a fork-join on four workers. They need the engine's
//! derived view, `determinize`, `apply` and the playout policy.

use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gcg_ai::{
    AgentCx, AgentError, Alternative, BASELINE_KNOBS, DEFAULT_WEIGHTS, MoveKey, NoopFlags,
    PolicyKnobs, PruneFlags, Scratch, canonical, first_player_choice, merge_equivalent,
    prune_dominated, without_effect,
};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{Action, ActionKind, GameState, StateSnapshot};
use serde_json::Value;

/// `knobs_for` of the adopted presets (`ai/config.py:60-77`, `ai/policy.py:63-69`).
const ADOPTED: PolicyKnobs = PolicyKnobs {
    burst_yes_score: 4.0,
    rule_polarity_table: true,
    zero_ap_face: true,
};

fn records() -> Vec<Value> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/search/decisions.ndjson");
    fs::read_to_string(path)
        .expect("the vectors")
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON record"))
        .collect()
}

fn state(reg: &Arc<Registry>, rec: &Value) -> GameState {
    let snap = &rec["snapshot"];
    let snapshot = StateSnapshot {
        state: snap["state"].clone(),
        continuous: snap["continuous"].clone(),
        filter_sets: snap["filter_sets"].clone(),
        rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
    };
    from_canonical(reg, &snapshot).unwrap_or_else(|e| panic!("{}: {e}", rec["id"]))
}

fn u64_of(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("not a u64: {v}"))
}

fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("not an int: {v}"))
}

fn action(v: &Value) -> Action {
    let kind = ActionKind::from_value(v[0].as_str().expect("a kind")).expect("an action kind");
    let arg = |i: usize| i32::try_from(int(&v[i])).expect("an i32 argument");
    Action::new(kind, arg(1), arg(2), arg(3))
}

fn move_key(v: &Value) -> MoveKey {
    MoveKey {
        kind: ActionKind::from_value(v[0].as_str().expect("a kind")).expect("an action kind"),
        a: int(&v[1]),
        b: int(&v[2]),
        variant: i32::try_from(int(&v[3])).expect("an i32 variant"),
    }
}

fn keys(v: &Value) -> Vec<MoveKey> {
    v.as_array().expect("keys").iter().map(move_key).collect()
}

/// Python's sort order of key lists: `[kind value, a, b, variant]`.
fn sorted(mut keys: Vec<MoveKey>) -> Vec<MoveKey> {
    keys.sort_by(|x, y| {
        (x.kind.as_str(), x.a, x.b, x.variant).cmp(&(y.kind.as_str(), y.a, y.b, y.variant))
    });
    keys
}

/// `float.fromhex` for the forms `float.hex()` writes.
fn fhex(s: &str) -> f64 {
    let (neg, body) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let body = body.strip_prefix("0x").expect("a hex float");
    let (mantissa, exp) = body.split_once('p').expect("an exponent");
    let exp: i32 = exp.parse().expect("an int exponent");
    let (int_part, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut m = u64::from_str_radix(int_part, 16).expect("hex digits");
    for c in frac.chars() {
        m = m * 16 + u64::from(c.to_digit(16).expect("a hex digit"));
    }
    let shift = exp - 4 * i32::try_from(frac.len()).expect("short");
    let pow2 = if shift >= -1022 {
        f64::from_bits(u64::try_from(shift + 1023).expect("in range") << 52)
    } else {
        f64::from_bits(1u64 << u32::try_from(shift + 1074).expect("subnormal range"))
    };
    #[expect(clippy::cast_precision_loss, reason = "a 53-bit mantissa is exact")]
    let v = m as f64 * pow2;
    if neg { -v } else { v }
}

fn alternatives(v: &Value) -> Vec<(Action, u64, u32)> {
    v.as_array()
        .expect("alternatives")
        .iter()
        .map(|a| {
            (
                action(&a[0]),
                fhex(a[1].as_str().expect("a hex float")).to_bits(),
                u32::try_from(int(&a[2])).expect("visits"),
            )
        })
        .collect()
}

fn bits(alts: &[Alternative]) -> Vec<(Action, u64, u32)> {
    alts.iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect()
}

#[test]
fn pruning_matches_python_for_every_flag_set() {
    let reg = Registry::embedded();
    let mut pruned = 0;
    for rec in records() {
        let st = state(&reg, &rec);
        let dec = st.pending().expect("a pending decision");
        for (name, strict_attacks, rest_targets) in [
            ("ff", false, false),
            ("tf", true, false),
            ("ft", false, true),
            ("tt", true, true),
        ] {
            let moves = canonical(&st, &dec);
            let n = moves.len();
            let kept = prune_dominated(
                &st,
                &dec,
                moves,
                PruneFlags {
                    strict_attacks,
                    rest_targets,
                },
            )
            .unwrap_or_else(|e| panic!("{}: {e}", rec["id"]));
            let got: Vec<MoveKey> = kept.iter().map(|&(k, _)| k).collect();
            assert_eq!(got, keys(&rec["prune"][name]), "{} {name}", rec["id"]);
            pruned += usize::from(got.len() < n);
        }
    }
    assert!(pruned > 0);
}

#[test]
fn merging_matches_python() {
    let reg = Registry::embedded();
    let mut merged = 0;
    for rec in records() {
        let st = state(&reg, &rec);
        let dec = st.pending().expect("a pending decision");
        let moves = canonical(&st, &dec);
        let n = moves.len();
        let out =
            merge_equivalent(&st, &dec, moves).unwrap_or_else(|e| panic!("{}: {e}", rec["id"]));
        let got: Vec<MoveKey> = out.iter().map(|&(k, _)| k).collect();
        assert_eq!(got, keys(&rec["merge"]), "{}", rec["id"]);
        merged += usize::from(got.len() < n);
    }
    assert!(merged > 0);
}

#[test]
fn moves_without_effect_match_python() {
    let reg = Registry::embedded();
    let mut idle = 0;
    for rec in records() {
        let Some(cases) = rec["idle"].as_array() else {
            continue;
        };
        let st = state(&reg, &rec);
        let dec = st.pending().expect("a pending decision");
        let player = dec.hdr.player;
        let moves = canonical(&st, &dec);
        for case in cases {
            let flags = case["flags"].as_bool().expect("flags");
            let knobs = if case["knobs"] == "adopted" {
                ADOPTED
            } else {
                BASELINE_KNOBS
            };
            let got = without_effect(
                &st,
                player,
                &dec,
                &moves,
                u64_of(&case["seed"]),
                &knobs,
                NoopFlags {
                    branch_choices: flags,
                    resource_aware: flags,
                },
                DEFAULT_WEIGHTS,
            )
            .unwrap_or_else(|e| panic!("{}: {e}", rec["id"]));
            assert_eq!(sorted(got.clone()), keys(&case["keys"]), "{}", rec["id"]);
            idle += usize::from(!got.is_empty());
        }
    }
    assert!(idle > 0);
}

fn play_draw(
    st: &GameState,
    rec: &Value,
    cx: &mut AgentCx<'_>,
) -> Result<(Action, Vec<Alternative>, u64), AgentError> {
    let fp = &rec["first_player"];
    let player = u8::try_from(int(&rec["player"])).expect("a seat");
    let games = u32::try_from(int(&fp["games"])).expect("games");
    let mut rng = SplitMix64::new(u64_of(&rec["seed"]));
    let (a, alts) = first_player_choice(st, player, games, &mut rng, &ADOPTED, cx)?;
    Ok((a, alts, rng.state))
}

#[test]
fn play_draw_choices_match_python_serially_and_as_a_fork_join() {
    let reg = Registry::embedded();
    let mut checked = 0;
    for rec in records() {
        if rec["kind"] != "choose_first" {
            continue;
        }
        let st = state(&reg, &rec);
        let cancel = Cancel::new();
        let serial = Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let mut cx = AgentCx {
                scratch,
                par,
                cancel: &cancel,
            };
            play_draw(&st, &rec, &mut cx)
        })
        .unwrap_or_else(|e| panic!("{}: {e}", rec["id"]));
        let fp = &rec["first_player"];
        assert_eq!(serial.0, action(&fp["action"]), "{}", rec["id"]);
        assert_eq!(bits(&serial.1), alternatives(&fp["alts"]), "{}", rec["id"]);
        assert_eq!(serial.2, u64_of(&fp["rng_after"]), "{}", rec["id"]);

        // A pool task must be `Sync`; the state (a `RefCell` view cache) is not, so each task
        // takes its own clone.
        let shared = Mutex::new(st.clone());
        let pool = Pool::new(NonZeroUsize::new(4).expect("four"));
        let parallel = pool
            .map_indexed(
                1,
                &cancel,
                |_, scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
                    let mut cx = AgentCx {
                        scratch,
                        par,
                        cancel: &cancel,
                    };
                    let st = shared.lock().expect("not poisoned").clone();
                    play_draw(&st, &rec, &mut cx)
                },
                |_| {},
            )
            .unwrap_or_else(|_| panic!("{}: the pool run failed", rec["id"]));
        let parallel = &parallel[0];
        assert_eq!(
            (parallel.0, bits(&parallel.1), parallel.2),
            (serial.0, bits(&serial.1), serial.2),
            "{}",
            rec["id"]
        );
        checked += 1;
    }
    assert!(checked >= 3, "{checked}");
}
