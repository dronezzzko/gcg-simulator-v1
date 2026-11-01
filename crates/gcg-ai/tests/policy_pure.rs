//! The engine-free parts of the playout policy and the evaluation against Python
//! (`pure.ndjson` of `port/oracle/export_policy_vectors.py`): `pick` (Gumbel-max sampling with
//! one draw per option, argmax with the first maximum, the `rng.random() or 1e-300` case) and
//! `logistic`, bit for bit (docs/port/inventory/08-ai-policy-eval.md §2.4, §6.9, §11).

mod policy_support;

use gcg_ai::{logistic, pick};
use gcg_engine::{Action, ActionKind};
use policy_support::{
    Mismatches, f, floats, from_hex, py_hex, records, rng_of, same, same_floats, u64_of,
};

#[test]
fn float_hex_round_trips_pythons_forms() {
    for (text, x) in [
        ("0x1.0000000000000p+0", 1.0),
        ("0x1.0000000000000p-1", 0.5),
        ("0x0.0p+0", 0.0),
        ("-0x0.0p+0", -0.0),
        ("0x0.0000000000001p-1022", 5e-324),
        ("0x1.0000000000000p-1022", 2.2250738585072014e-308),
        ("0x1.8000000000000p+0", 1.5),
        ("0x1.c2f7d5a8a79c9p-1", 0.8807970779778823),
        ("0x1.fffffffffffffp+1023", f64::MAX),
        ("inf", f64::INFINITY),
        ("-inf", f64::NEG_INFINITY),
    ] {
        assert_eq!(from_hex(text).to_bits(), x.to_bits(), "{text}");
        assert_eq!(py_hex(x), text);
    }
}

#[test]
fn pick_matches_python_draw_for_draw() {
    let recs: Vec<_> = records("pure.ndjson")
        .into_iter()
        .filter(|r| r["t"] == "pick")
        .collect();
    let mut m = Mismatches::default();
    let mut sampled = 0;
    for (i, r) in recs.iter().enumerate() {
        let scores = floats(&r["scores"]);
        let options: Vec<Action> = (0..scores.len())
            .map(|k| Action::new(ActionKind::Pass, i32::try_from(k).unwrap(), -1, -1))
            .collect();
        let temperature = f(&r["temperature"]);
        let mut rng = (!r["seed"].is_null()).then(|| rng_of(&r["seed"]));
        let got = pick(&options, &scores, rng.as_mut(), temperature);
        let id = format!("pick #{i}");
        m.check(
            &id,
            "index",
            same(got, usize::try_from(r["index"].as_u64().unwrap()).unwrap()),
        );
        if let Some(rng) = rng {
            sampled += 1;
            m.check(&id, "rng after", same(rng.state, u64_of(&r["rng_after"])));
        }
    }
    m.assert_none(400);
    assert!(sampled >= 250, "only {sampled} seeded cases");
}

/// Inv 08 §11.1: `[1.0, 2.0, 0.5, 2.0]`, `SplitMix64(42)`, `T = 0.5` picks index 3 after four
/// draws; without a generator the first of the tied maxima (index 1).
#[test]
fn pick_reproduces_the_inventory_vector() {
    let scores = [1.0, 2.0, 0.5, 2.0];
    let options = [Action::of(ActionKind::Pass); 4];
    let mut rng = gcg_core::rng::SplitMix64::new(42);
    assert_eq!(pick(&options, &scores, Some(&mut rng), 0.5), 3);
    assert_eq!(rng.state, 8_709_371_129_873_690_750);
    assert_eq!(pick(&options, &scores, None, 0.5), 1);
    let mut rng = gcg_core::rng::SplitMix64::new(42);
    assert_eq!(pick(&options, &scores, Some(&mut rng), 0.0), 1);
    assert_eq!(rng.state, 42, "a zero temperature draws nothing");
}

#[test]
fn logistic_matches_python_bit_for_bit() {
    let mut m = Mismatches::default();
    for r in records("pure.ndjson")
        .iter()
        .filter(|r| r["t"] == "logistic")
    {
        let z = f(&r["z"]);
        m.check(
            &py_hex(z),
            "logistic",
            same_floats(&[logistic(z)], &[f(&r["value"])]),
        );
    }
    m.assert_none(250);
    assert_eq!(logistic(-0.0).to_bits(), 0.5f64.to_bits());
    assert_eq!(logistic(800.0), 1.0);
    assert_eq!(logistic(-800.0), 0.0);
}
