//! `opening_score(hand, first)` (`ai/setup.py:30-60`) on the sampled hands of
//! `opening.ndjson` (`port/oracle/export_policy_vectors.py`), bit for bit. The function is
//! D2's (`setup.rs`); D1 exports its vectors with the policy set (PLAN §10.4 D1).

mod policy_support;

use gcg_ai::opening_score;
use gcg_data::{CardDef, DefId};
use policy_support::{Mismatches, f, records, registry, same_floats};

#[test]
fn opening_score_matches_python() {
    let reg = registry();
    let db = reg.db();
    let mut m = Mismatches::default();
    for (i, r) in records("opening.ndjson").iter().enumerate() {
        let hand: Vec<&CardDef> = r["hand"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| db.by_id(DefId(u16::try_from(d.as_u64().unwrap()).unwrap())))
            .collect();
        let first = r["first"].as_bool().unwrap();
        m.check(
            &format!("hand #{i}"),
            "opening_score",
            same_floats(&[opening_score(&hand, first, &reg)], &[f(&r["score"])]),
        );
    }
    m.assert_none(300);
}
