//! Decoding of every record type, the per-test records included (PLAN §8.2), in the form
//! `oracle/pytest_trace.py` (workstream G2) writes them. The `call` arguments cover one
//! synthetic value per argument kind (T-157, the Rust side; the Python encoder's own round
//! trip is `pytest_trace.py --selftest`).

use gcg_data::DefId;
use gcg_diff::{
    DerivedMark, EngineRef, EnumValue, OverrideKind, Record, TestOutcome, TraceArg, parse_record,
};
use gcg_effects::{AbilityId, ProgramId};

#[test]
fn call_arguments_decode_one_per_kind() {
    let line = r#"{"after16":"0123456789abcdef","args":[null,true,-3,"GD01-001",[1,[2,"x"]],{"enum":"Zone","value":8},{"enum":"Duration","value":"this_turn"},{"$":17},"0x1.999999999999ap-4","1152921504606846976"],"error":null,"fn":"interp.add_lasting","kwargs":[["by",1],["filters",[{"$":4}]]],"result":[1,2],"t":"call"}"#;
    let Ok(Record::Call(c)) = parse_record(line) else {
        panic!("not a call: {:?}", parse_record(line))
    };
    assert_eq!(c.call.func, "interp.add_lasting");
    assert_eq!(
        c.call.args,
        vec![
            TraceArg::Null,
            TraceArg::Bool(true),
            TraceArg::Int(-3),
            TraceArg::Str("GD01-001".to_owned()),
            TraceArg::List(vec![
                TraceArg::Int(1),
                TraceArg::List(vec![TraceArg::Int(2), TraceArg::Str("x".to_owned())]),
            ]),
            TraceArg::Enum {
                name: "Zone".to_owned(),
                value: EnumValue::Int(8),
            },
            TraceArg::Enum {
                name: "Duration".to_owned(),
                value: EnumValue::Str("this_turn".to_owned()),
            },
            TraceArg::Node(17),
            TraceArg::Str("0x1.999999999999ap-4".to_owned()),
            TraceArg::Str("1152921504606846976".to_owned()),
        ]
    );
    assert_eq!(
        c.call.kwargs,
        vec![
            ("by".to_owned(), TraceArg::Int(1)),
            (
                "filters".to_owned(),
                TraceArg::List(vec![TraceArg::Node(4)])
            ),
        ]
    );
    assert!(c.new_game.is_none() && c.error.is_none());
    assert!(
        parse_record(&line.replace("-3", "-3.5")).is_err(),
        "floats are float.hex() strings, never JSON numbers"
    );
    assert!(parse_record(&line.replace(r#"{"$":17}"#, r#"{"$":17,"x":1}"#)).is_err());
    assert!(
        parse_record(&line.replace(r#"[["by",1],"#, r#"{"by":1},["#)).is_err(),
        "kwargs are [name, argument] pairs"
    );
}

#[test]
fn engine_references_decode_and_are_checked() {
    let call = |args: &str| {
        format!(
            r#"{{"after16":"0123456789abcdef","args":[{args}],"error":null,"fn":"interp.push_frame","result":null,"t":"call"}}"#
        )
    };
    let refs = r#"{"enum":"GameState","value":"ef06f3a7edc457dc"},{"enum":"GameState","value":"scenario"},{"enum":"Derived","value":"current"},{"enum":"CardDef","value":4},{"enum":"AbilityEntry","value":1261},{"enum":"Frame","value":-1},{"enum":"Program","value":627},{"enum":"Registry","value":"current"},[0,-1,5,[["x",[0]]],[]],{"context":[["min",1]],"kind":"main","options":[["end_main",-1,-1,-1]],"player":0,"prompt":"main phase"}"#;
    let Ok(Record::Call(c)) = parse_record(&call(refs)) else {
        panic!("refs: {:?}", parse_record(&call(refs)))
    };
    let got: Vec<&TraceArg> = c.call.args.iter().take(8).collect();
    assert_eq!(
        got,
        [
            &TraceArg::Ref(EngineRef::State("ef06f3a7edc457dc".to_owned())),
            &TraceArg::Ref(EngineRef::ScenarioState),
            &TraceArg::Ref(EngineRef::View),
            &TraceArg::Ref(EngineRef::CardDef(DefId(4))),
            &TraceArg::Ref(EngineRef::Ability(AbilityId(1261))),
            &TraceArg::Ref(EngineRef::Frame(-1)),
            &TraceArg::Ref(EngineRef::Program(ProgramId(627))),
            &TraceArg::Ref(EngineRef::Registry),
        ]
    );
    assert!(matches!(&c.call.args[8], TraceArg::List(ctx) if ctx.len() == 5));
    assert!(matches!(&c.call.args[9], TraceArg::Decision(d) if d.prompt == "main phase"));
    for bad in [
        r#"{"enum":"GameState","value":"xyz"}"#,
        r#"{"enum":"Derived","value":"2cfbdf926a7ca8f9"}"#,
        r#"{"enum":"Registry","value":1}"#,
        r#"{"enum":"CardDef","value":-1}"#,
        r#"{"enum":"Frame","value":-2}"#,
        r#"{"enum":"Program","value":"627"}"#,
        r#"{"enum":"Zone","value":7,"extra":1}"#,
        r#"{"context":[],"kind":"main","player":0,"prompt":"p"}"#,
        r#"{"other":1}"#,
    ] {
        assert!(parse_record(&call(bad)).is_err(), "{bad} must not decode");
    }
}

#[test]
fn call_names_follow_the_recorded_modules() {
    let probe = |func: &str| format!(r#"{{"args":[5],"fn":"{func}","result":3,"t":"probe"}}"#);
    let call = |func: &str| {
        format!(
            r#"{{"after16":"0123456789abcdef","args":[],"error":null,"fn":"{func}","result":null,"t":"call"}}"#
        )
    };
    assert!(parse_record(&probe("view.ap_of")).is_ok());
    assert!(parse_record(&probe("testkit.zone_of")).is_ok());
    assert!(
        parse_record(&probe("views.cards")).is_ok(),
        "traces.py generate_view"
    );
    assert!(
        parse_record(&probe("V.trigger_index")).is_ok(),
        "traces.py generate_view"
    );
    assert!(parse_record(&probe("views.other")).is_err());
    assert!(parse_record(&call("game.advance")).is_ok());
    assert!(parse_record(&call("core.clear_listen_cache")).is_ok());
    for bad in ["V.ap_of", "view.ap of", "view.", "ap_of"] {
        assert!(parse_record(&probe(bad)).is_err(), "probe name {bad}");
    }
    assert!(
        parse_record(&probe("core.destroy")).is_err(),
        "core is a call module"
    );
    assert!(
        parse_record(&call("view.cdef")).is_err(),
        "view is a probe module"
    );
    assert!(
        parse_record(&call("Scenario.add")).is_err(),
        "Scenario only in a scenario"
    );
}

const SNAP: &str = r#""continuous":{"0123456789abcdef":{"_":"StatMod","ap":1,"hp":0}},"filter_sets":{},"rules_checked":-1,"state":{"once_used":[]}"#;

fn scenario_line(extra: &str, calls: &str) -> String {
    format!(
        r#"{{"after16":"1111111111111111","before16":"2222222222222222","calls":[{calls}],"calls_reproduce":true,"new_game":null,{extra}"t":"scenario","tags":["rule:8-5-2","card:GD01-060"],"test":"tests/rules/test_x.py::test_y"}}"#
    )
}

const CALLS: &str = r#"{"args":[],"fn":"Scenario.__init__"},{"args":[0,"GD01-060"],"fn":"Scenario.add","kwargs":[["rested",true]]},{"args":[0,"GD01-999"],"error":{"message":"'GD01-999'","type":"KeyError"},"fn":"Scenario.add"},{"args":[{"enum":"GameState","value":"scenario"},{"enum":"Program","value":2}],"fn":"interp.push_frame"},{"args":[],"fn":"Scenario.start"}"#;

#[test]
fn scenario_records_decode() {
    let rc = r#""rules_checked":-1,"#;
    let Ok(Record::Scenario(s)) = parse_record(&scenario_line(rc, CALLS)) else {
        panic!("scenario: {:?}", parse_record(&scenario_line(rc, CALLS)))
    };
    assert_eq!(s.calls.len(), 5);
    assert_eq!(
        s.calls[2].error.as_ref().map(|e| e.class.as_str()),
        Some("KeyError")
    );
    assert_eq!(
        s.calls[3].call.args[0],
        TraceArg::Ref(EngineRef::ScenarioState)
    );
    assert_eq!(s.before16.as_deref(), Some("2222222222222222"));
    assert_eq!(s.after16, "1111111111111111");
    assert!(s.calls_reproduce && s.snapshot.is_none() && s.new_game.is_none());
    assert_eq!(s.rules_checked, -1);

    let with_state = scenario_line(&format!("{SNAP},"), CALLS);
    let Ok(Record::Scenario(s)) = parse_record(&with_state) else {
        panic!("with state: {:?}", parse_record(&with_state))
    };
    assert!(s.snapshot.is_some());
    let not_reproduced =
        with_state.replace(r#""calls_reproduce":true"#, r#""calls_reproduce":false"#);
    assert!(parse_record(&not_reproduced).is_ok());
    let not_reproduced_without_state =
        scenario_line(rc, CALLS).replace(r#""calls_reproduce":true"#, r#""calls_reproduce":false"#);
    assert!(
        parse_record(&not_reproduced_without_state).is_err(),
        "calls that do not reproduce the state need the snapshot"
    );
    assert!(
        parse_record(&scenario_line("", CALLS)).is_err(),
        "rules_checked is always present"
    );
    let continuous_only = scenario_line(r#""continuous":{},"rules_checked":-1,"#, CALLS);
    assert!(
        parse_record(&continuous_only).is_err(),
        "continuous without a state"
    );
    let no_start = CALLS.replace(r#",{"args":[],"fn":"Scenario.start"}"#, "");
    assert!(parse_record(&scenario_line(rc, &no_start)).is_err());
    let no_init = CALLS.replace(r#"{"args":[],"fn":"Scenario.__init__"},"#, "");
    assert!(parse_record(&scenario_line(rc, &no_init)).is_err());
    assert!(
        parse_record(&scenario_line(rc, CALLS).replace(r#","before16":"2222222222222222""#, ""))
            .is_err(),
        "a Scenario record has before16"
    );
    assert!(
        parse_record(&scenario_line(
            rc,
            &CALLS.replace("Scenario.add", "Scenario.bogus")
        ))
        .is_err()
    );
}

const DECKS: &str = r#""decks":[{"main":["A"],"res":["R-001"]},{"main":["B"],"res":["R-001"]}]"#;

#[test]
fn new_game_records_decode() {
    let spec = format!(
        r#"{{"chooser":null,{DECKS},"max_actions":20000,"seed":9,"turn_limit":200,"validate":true}}"#
    );
    let new_game = format!(
        r#"{{"after16":"1111111111111111","calls":[],"calls_reproduce":true,"new_game":{spec},"rules_checked":812,"t":"scenario","tags":[],"test":"t::u"}}"#
    );
    let Ok(Record::Scenario(s)) = parse_record(&new_game) else {
        panic!("new_game: {:?}", parse_record(&new_game))
    };
    assert_eq!(
        s.new_game
            .map(|n| (n.seed, n.turn_limit, n.max_actions, n.validate)),
        Some((9, 200, 20_000, true))
    );
    assert_eq!((s.before16, s.rules_checked), (None, 812));
    assert!(
        parse_record(&new_game.replace(r#","validate":true"#, "")).is_err(),
        "validate is always written"
    );
    assert!(
        parse_record(&new_game.replace(
            r#""calls":[]"#,
            r#""calls":[{"args":[],"fn":"Scenario.start"}]"#
        ))
        .is_err(),
        "a new_game record has no calls"
    );
    let failed = format!(
        r#"{{"after16":"","args":[],"error":{{"message":"player 0: deck has 49 cards; exactly 50 required (6-1-1)","type":"DeckConstructionError"}},"fn":"game.new_game","new_game":{spec},"result":null,"t":"call"}}"#
    );
    let Ok(Record::Call(c)) = parse_record(&failed) else {
        panic!("failed new_game: {:?}", parse_record(&failed))
    };
    assert!(c.new_game.is_some() && c.after16.is_empty());
    assert_eq!(
        c.error.map(|e| e.class),
        Some("DeckConstructionError".to_owned())
    );
    assert!(
        parse_record(&failed.replace(r#""fn":"game.new_game""#, r#""fn":"game.advance""#)).is_err(),
        "only game.new_game carries a spec"
    );
    assert!(
        parse_record(&failed.replace(r#""after16":"""#, r#""after16":"abc""#)).is_err(),
        "after16 is 16 hex digits or empty"
    );
}

#[test]
fn per_test_records_decode() {
    let apply = r#"{"action":["select",3,-1,-1],"after16":"1111111111111111","before16":"2222222222222222","check":true,"error":{"message":"illegal action select(3) for main","type":"IllegalActionError"},"pending":{"context":[["min",1]],"kind":"main","options":[["end_main",-1,-1,-1]],"player":0,"prompt":"main phase"},"t":"apply"}"#;
    let Ok(Record::Apply(a)) = parse_record(apply) else {
        panic!("apply: {:?}", parse_record(apply))
    };
    assert_eq!(
        a.error.map(|e| e.class),
        Some("IllegalActionError".to_owned())
    );
    let stale = format!(
        r#"{{{SNAP},"derived":"stale","t":"state","view_from":{{"once_used":[]}},"view_key":[0,3,0,[],5]}}"#
    );
    let Ok(Record::State(st)) = parse_record(&stale) else {
        panic!("state: {:?}", parse_record(&stale))
    };
    assert_eq!(st.derived, DerivedMark::Stale);
    let fresh_with_view = stale.replace(r#""derived":"stale""#, r#""derived":"fresh""#);
    assert!(
        parse_record(&fresh_with_view).is_err(),
        "view_from only with a stale view"
    );
    let Ok(Record::View(v)) = parse_record(r#"{"sig":{"ap":[]},"t":"view"}"#) else {
        panic!("view")
    };
    assert!(v.error.is_none() && v.sig.is_object());
    let Ok(Record::View(v)) =
        parse_record(r#"{"error":{"message":"boom","type":"EngineError"},"sig":null,"t":"view"}"#)
    else {
        panic!("failed view")
    };
    assert!(v.sig.is_null() && v.error.is_some());
    assert!(parse_record(r#"{"sig":null,"t":"view"}"#).is_err());
    assert!(parse_record(r#"{"error":{"message":"m","type":"T"},"sig":{},"t":"view"}"#).is_err());
    let Ok(Record::Probe(p)) =
        parse_record(r#"{"args":[5],"fn":"view.ap_of","result":3,"t":"probe"}"#)
    else {
        panic!("probe")
    };
    assert!(p.error.is_none());
    let failed = r#"{"args":[5],"error":{"message":"5","type":"IndexError"},"fn":"testkit.ap","result":null,"t":"probe"}"#;
    assert!(matches!(parse_record(failed), Ok(Record::Probe(p)) if p.error.is_some()));
    assert!(parse_record(&failed.replace(r#""result":null"#, r#""result":3"#)).is_err());
    assert_eq!(
        parse_record(r#"{"t":"force_link","units":["GD05-035"]}"#),
        Ok(Record::ForceLink(vec!["GD05-035".to_owned()]))
    );
    assert_eq!(
        parse_record(r#"{"outcome":"passed","t":"end"}"#),
        Ok(Record::End(TestOutcome::Passed))
    );
    assert!(parse_record(r#"{"outcome":"skipped","t":"end"}"#).is_err());
    assert!(parse_record(r#"{"t":"bogus"}"#).is_err());
}

/// An empty fragment in `export_ir.build_fragment`'s format.
const FRAGMENT: &str = r#"{"abilities":[],"aliases":[],"always_events":["destroys_by_battle","turn_end"],"base_fingerprint":"a61a7e50899ab6ff6259b0a2a082be5af45cf3fa28b5012c95597a789688748b","cards":[],"continuous":[],"events":[],"filter_sets":[],"fragment_version":1,"hp_reduction_possible":false,"node_content_ids":["1c9bcdb2bfdeb203"],"nodes":[{"_":"HasTrait","traits":["Neo Zeon"]}],"printable_non_ascii":[],"printed_keywords":[],"programs":[{"content_id":"0123456789abcdef","instrs":[4477],"label":"test"}],"rule_mods":[],"strings":["Neo Zeon","test"],"var_names":[]}"#;

#[test]
fn override_records_decode() {
    let additions = format!(r#"{{"fragment":{FRAGMENT},"kind":"additions","t":"override"}}"#);
    let Ok(Record::Override(o)) = parse_record(&additions) else {
        panic!("additions: {:?}", parse_record(&additions))
    };
    assert_eq!(o.kind, OverrideKind::Additions);
    assert!(o.memo.is_none() && o.fragment["fragment_version"] == 1);
    let memo = r#"{"empty":false,"reads":[{"key":["0123456789abcdef",3],"table":"gcg_sim.ai.actions._rest_only","value":true}],"sizes":{"gcg_sim.ai.actions._rest_only":1,"gcg_sim.ai.targeting._cache":0}}"#;
    let over =
        format!(r#"{{"fragment":{FRAGMENT},"kind":"override","memo":{memo},"t":"override"}}"#);
    let Ok(Record::Override(o)) = parse_record(&over) else {
        panic!("override: {:?}", parse_record(&over))
    };
    let m = o.memo.expect("memo");
    assert!(!m.empty && m.reads.len() == 1 && m.sizes.len() == 2);
    let restore = format!(r#"{{"fragment":{FRAGMENT},"kind":"restore","t":"override"}}"#);
    assert!(
        matches!(parse_record(&restore), Ok(Record::Override(o)) if o.kind == OverrideKind::Restore)
    );
    assert!(
        parse_record(&format!(
            r#"{{"fragment":{FRAGMENT},"kind":"override","t":"override"}}"#
        ))
        .is_err(),
        "an override carries its memo check"
    );
    assert!(
        parse_record(&over.replace(r#""empty":false"#, r#""empty":true"#)).is_err(),
        "empty exactly when every table is"
    );
    for (from, to) in [
        (r#""fragment_version":1"#, r#""fragment_version":2"#),
        (r#""kind":"additions""#, r#""kind":"bogus""#),
        (r#""rule_mods":[],"#, ""),
        (r#""rule_mods":[],"#, r#""rule_mods":[],"memo":{},"#),
        (
            r#""node_content_ids":["1c9bcdb2bfdeb203"]"#,
            r#""node_content_ids":[]"#,
        ),
        (r#""base_fingerprint":"a61a"#, r#""base_fingerprint":"zz1a"#),
    ] {
        let bad = additions.replace(from, to);
        assert_ne!(bad, additions);
        assert!(parse_record(&bad).is_err(), "{to} must not decode");
    }
}
