//! The replay of the per-test records (PLAN §8.2, §8.7) on a synthetic trace in the form
//! `oracle/pytest_trace.py` writes: the `MockEngine` of the unmutated trace replays it
//! cleanly, and each mutated copy diverges at the mutated record and field (the scenario
//! build and its digests, a `calls_reproduce: false` snapshot, the pending decision before an
//! apply, errors of calls, probes and view signatures, and a failed `new_game`).

use gcg_diff::json::state_digest;
use gcg_diff::reader::parse_traces;
use gcg_diff::{DiffOptions, MockEngine, Trace, replay_trace};

const HEADER: &str = r#"{"agent_seeds":[],"agents":[],"chooser":null,"decks":null,"generator":{"code_digest":"c","data_lock":"d","effect_offset":0,"git":"g","hash_seed":1,"ir_sha256":"i","max_uid":3,"python":"3.15.1"},"id":"tests/rules/test_x.py::test_y","kind":"test","max_actions":20000,"options_mode":"full","preset":null,"seed":null,"t":"header","turn_limit":200,"v":1}"#;
const FRAGMENT: &str = r#"{"abilities":[],"aliases":[],"always_events":[],"base_fingerprint":"a61a7e50899ab6ff6259b0a2a082be5af45cf3fa28b5012c95597a789688748b","cards":[],"continuous":[],"events":[],"filter_sets":[],"fragment_version":1,"hp_reduction_possible":false,"node_content_ids":[],"nodes":[],"printable_non_ascii":[],"printed_keywords":[],"programs":[],"rule_mods":[],"strings":[],"var_names":[]}"#;
const PENDING: &str = r#"{"context":[],"kind":"main","options":[["end_main",-1,-1,-1]],"player":0,"prompt":"main phase"}"#;
const DECKS: &str = r#"{"chooser":null,"decks":[{"main":["A"],"res":["R-001"]},{"main":["B"],"res":["R-001"]}],"max_actions":20000,"seed":9,"turn_limit":200,"validate":true}"#;

fn lines() -> Vec<String> {
    let state = r#"{"once_used":[],"turn":3}"#;
    let pre = &state_digest(&serde_json::from_str(state).expect("JSON")).expect("digest")[..16];
    vec![
        HEADER.to_owned(),
        format!(r#"{{"fragment":{FRAGMENT},"kind":"additions","t":"override"}}"#),
        r#"{"after16":"1111111111111111","before16":"2222222222222222","calls":[{"args":[],"fn":"Scenario.__init__"},{"args":[0,"X"],"error":{"message":"'X'","type":"KeyError"},"fn":"Scenario.add"},{"args":[],"fn":"Scenario.start"}],"calls_reproduce":true,"new_game":null,"rules_checked":-1,"t":"scenario","tags":[],"test":"tests/rules/test_x.py::test_y"}"#.to_owned(),
        format!(r#"{{"action":["end_main",-1,-1,-1],"after16":"3333333333333333","before16":"1111111111111111","check":true,"error":null,"pending":{PENDING},"t":"apply"}}"#),
        r#"{"error":{"message":"boom","type":"KeyError"},"sig":null,"t":"view"}"#.to_owned(),
        r#"{"args":[9],"error":{"message":"9","type":"IndexError"},"fn":"testkit.ap","result":null,"t":"probe"}"#.to_owned(),
        r#"{"after16":"3333333333333333","args":[{"enum":"GameState","value":"3333333333333333"}],"error":null,"fn":"game.advance","result":null,"t":"call"}"#.to_owned(),
        format!(r#"{{"after16":"3333333333333333","args":[],"error":{{"message":"bad deck","type":"DeckConstructionError"}},"fn":"game.new_game","new_game":{DECKS},"result":null,"t":"call"}}"#),
        format!(r#"{{"after16":"4444444444444444","calls":[],"calls_reproduce":true,"new_game":{DECKS},"rules_checked":7,"t":"scenario","tags":[],"test":"tests/rules/test_x.py::test_y"}}"#),
        format!(r#"{{"after16":"5555555555555555","before16":"{pre}","calls":[{{"args":[],"fn":"Scenario.__init__"}},{{"args":[],"fn":"Scenario.start"}}],"calls_reproduce":false,"continuous":{{}},"filter_sets":{{}},"new_game":null,"rules_checked":4,"state":{state},"t":"scenario","tags":[],"test":"tests/rules/test_x.py::test_y"}}"#),
        r#"{"outcome":"passed","t":"end"}"#.to_owned(),
    ]
}

fn trace(lines: &[String]) -> Trace {
    let text = lines.join("\n") + "\n";
    let mut traces = parse_traces(&text).unwrap_or_else(|(n, e)| panic!("line {n}: {e}"));
    assert_eq!(traces.len(), 1);
    traces.remove(0)
}

/// Replays `mutated` against the mock of the original and returns the divergence's line and
/// field.
fn diverge(edit: impl FnOnce(&mut Vec<String>)) -> (usize, String) {
    let original = trace(&lines());
    let mut l = lines();
    edit(&mut l);
    let mut mock = MockEngine::new(&original);
    let d = replay_trace(&trace(&l), &DiffOptions::default(), &mut mock).expect_err("diverges");
    (d.line, d.field)
}

#[test]
fn the_unmutated_trace_replays() {
    let t = trace(&lines());
    let mut mock = MockEngine::new(&t);
    let stats = replay_trace(&t, &DiffOptions::default(), &mut mock).expect("replays");
    assert_eq!((stats.applies, stats.probes), (1, 2));
    assert_eq!(
        stats.calls,
        3 + 2 + 1,
        "scenario calls, call records, the snapshot's advance"
    );
}

#[test]
fn mutated_per_test_records_diverge_at_their_record() {
    let (line, field) = diverge(|l| l[2] = l[2].replace("1111111111111111", "1111111111111112"));
    assert_eq!((line, field.as_str()), (3, "scenario.after16"));
    let (line, field) = diverge(|l| l[2] = l[2].replace("2222222222222222", "2222222222222223"));
    assert_eq!(line, 3);
    assert!(
        field.starts_with("scenario Scenario.start before advance"),
        "{field}"
    );
    let (line, field) = diverge(|l| {
        l[2] = l[2].replace(r#""error":{"message":"'X'","type":"KeyError"},"#, "");
    });
    assert_eq!((line, field.as_str()), (3, "scenario Scenario.add"));
    let (line, field) = diverge(|l| l[3] = l[3].replace(r#""player":0"#, r#""player":1"#));
    assert_eq!((line, field.as_str()), (4, "apply.pending"));
    let (line, field) = diverge(|l| {
        l[4] = r#"{"sig":{"ap":[]},"t":"view"}"#.to_owned();
    });
    assert_eq!((line, field.as_str()), (5, "view paranoid.view_signature"));
    let (line, field) = diverge(|l| l[5] = l[5].replace("IndexError", "KeyError"));
    assert_eq!((line, field.as_str()), (6, "probe testkit.ap"));
    let (line, field) = diverge(|l| l[7] = l[7].replace("bad deck", "other"));
    assert_eq!((line, field.as_str()), (8, "call game.new_game"));
    let (line, field) =
        diverge(|l| l[8] = l[8].replace(r#""rules_checked":7"#, r#""rules_checked":8"#));
    assert_eq!((line, field.as_str()), (9, "scenario.rules_checked"));
    let (line, field) = diverge(|l| l[9] = l[9].replace(r#""turn":3"#, r#""turn":4"#));
    assert_eq!((line, field.as_str()), (10, "scenario.before16"));
}
