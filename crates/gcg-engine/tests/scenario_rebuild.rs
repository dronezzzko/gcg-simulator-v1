//! T-057, second part: every `scenario` record of the per-test traces
//! (`oracle/corpus/v1/tier1/tests/**`, PLAN §8.2, §8.7) rebuilds through the Rust testkit alone,
//! without `gcg-diff`'s dispatcher.
//!
//! - A Scenario build (`testkit.py:38-182`) replays its recorded calls on
//!   [`Scenario`] over the registry the test ran on (the embedded IR, or it with the test's
//!   override fragment), with the `force_link` units recorded before it. The state before
//!   `Scenario.start`'s `advance` ([`Scenario::prepare_start`]) has the recorded `before16`
//!   digest prefix and `rules_checked`, and the state after `advance` has `after16`.
//! - A build whose test wrote the Scenario's state between calls (`calls_reproduce: false`)
//!   still replays its calls without error; its recorded pre-advance snapshot loads with
//!   `before16`, and `advance` from it gives `after16`.
//! - A test that started with `new_game` gets `after16` and `rules_checked` from
//!   [`new_game_by_number`] with the recorded decks, seed, chooser, caps and `validate`.
//!
//! Every record is checked; the counts are printed and the totals asserted.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gcg_effects::{ProgramId, Registry};
use gcg_engine::canon::{from_canonical, state_digest};
use gcg_engine::testkit::engine as tk;
use gcg_engine::{
    AddOptions, EventPayload, FrameArgs, FrameKind, GameState, NewGameOptions, Scenario,
    ScenarioOptions, StateSnapshot, Step, Zone, new_game_by_number,
};
use serde_json::Value;

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/corpus/v1/tier1/tests")
}

fn trace_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("a directory entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            trace_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "ndjson") {
            out.push(p);
        }
    }
}

fn digest16(st: &GameState) -> String {
    state_digest(st)[..16].to_owned()
}

fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("not an int: {v}"))
}

fn u64_of(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("not a u64: {v}"))
}

fn small<T: TryFrom<i64>>(v: &Value) -> T {
    T::try_from(int(v)).unwrap_or_else(|_| panic!("out of range: {v}"))
}

/// `{"enum": name, "value": v}` (the trace argument encoding of PLAN §8.2).
fn enum_value<'v>(v: &'v Value, name: &str) -> &'v Value {
    assert_eq!(v["enum"], name, "expected a {name}: {v}");
    &v["value"]
}

fn strings(args: &[Value]) -> Vec<&str> {
    args.iter()
        .map(|a| {
            a.as_str()
                .unwrap_or_else(|| panic!("not a card number: {a}"))
        })
        .collect()
}

/// A recorded call's keyword arguments (`[[name, value], ...]`).
struct Kwargs<'v>(Vec<(&'v str, &'v Value)>);

impl<'v> Kwargs<'v> {
    fn of(call: &'v Value) -> Self {
        let pairs = call
            .get("kwargs")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        Kwargs(
            pairs
                .iter()
                .map(|kv| (kv[0].as_str().expect("a keyword"), &kv[1]))
                .collect(),
        )
    }

    fn get(&self, k: &str) -> Option<&'v Value> {
        self.0.iter().rev().find(|(n, _)| *n == k).map(|(_, v)| *v)
    }

    fn flag(&self, k: &str, d: bool) -> bool {
        self.get(k).map_or(d, |v| v.as_bool().expect("a bool"))
    }

    fn int_or(&self, k: &str, d: i64) -> i64 {
        self.get(k).map_or(d, int)
    }

    fn only(&self, allowed: &[&str], f: &str) {
        for (k, _) in &self.0 {
            assert!(allowed.contains(k), "{f}: unexpected keyword {k}");
        }
    }
}

/// One call of a Scenario build: a `Scenario` method or `interp.push_frame` on the Scenario's
/// state (`testkit.py:38-182`, the argument encoding of PLAN §8.2). `Scenario.start` runs
/// [`Scenario::prepare_start`] only; the caller checks the state, then advances.
fn scenario_call(sc: &mut Scenario, call: &Value) {
    let f = call["fn"].as_str().expect("a function name");
    let args = call["args"].as_array().expect("args").as_slice();
    let kw = Kwargs::of(call);
    let ok = |r: Result<(), gcg_engine::TestkitError>| {
        r.unwrap_or_else(|e| panic!("{f}{args:?}: {e:?}"));
    };
    match f {
        "Scenario.start" => {
            kw.only(&[], f);
            let step = args.first().map_or(Step::Main, |s| {
                Step::from_value(int(enum_value(s, "Step"))).expect("a step")
            });
            ok(sc.prepare_start(step));
        }
        "Scenario.add" => {
            kw.only(
                &["rested", "damage", "pilot", "deployed_this_turn", "known"],
                f,
            );
            let zone = args.get(2).map_or(Zone::Battle, |z| {
                Zone::from_value(int(enum_value(z, "Zone"))).expect("a zone")
            });
            let o = AddOptions {
                rested: kw.flag("rested", false),
                damage: i32::try_from(kw.int_or("damage", 0)).expect("a damage"),
                pilot: kw.get("pilot").and_then(Value::as_str),
                deployed_this_turn: kw.flag("deployed_this_turn", false),
                known: kw.flag("known", true),
            };
            let number = args[1].as_str().expect("a card number");
            ok(sc.add(small(&args[0]), number, zone, o).map(drop));
        }
        "Scenario.add_raw" => {
            kw.only(&[], f);
            let zone = Zone::from_value(int(enum_value(&args[2], "Zone"))).expect("a zone");
            let number = args[1].as_str().expect("a card number");
            ok(sc.add_raw(small(&args[0]), number, zone).map(drop));
        }
        "Scenario.hand" | "Scenario.trash" | "Scenario.shields" | "Scenario.deck" => {
            kw.only(&[], f);
            let p = small(&args[0]);
            let numbers = strings(&args[1..]);
            match f {
                "Scenario.hand" => ok(sc.hand(p, &numbers).map(drop)),
                "Scenario.trash" => ok(sc.trash(p, &numbers).map(drop)),
                "Scenario.shields" => ok(sc.shields(p, &numbers).map(drop)),
                _ => sc.deck(p, &numbers),
            }
        }
        "Scenario.resources" => {
            kw.only(&["rested", "ex"], f);
            let count = |k: &str| usize::try_from(kw.int_or(k, 0)).expect("a count");
            let n = usize::try_from(int(&args[1])).expect("a count");
            ok(sc
                .resources(small(&args[0]), n, count("rested"), count("ex"))
                .map(drop));
        }
        "Scenario.base" => {
            kw.only(&["damage"], f);
            let number = args.get(1).and_then(Value::as_str);
            let damage = i32::try_from(kw.int_or("damage", 0)).expect("a damage");
            ok(sc.base(small(&args[0]), number, damage).map(drop));
        }
        "Scenario.resource_deck" => {
            kw.only(&[], f);
            let n = usize::try_from(int(&args[1])).expect("a count");
            ok(sc.resource_deck(small(&args[0]), n));
        }
        "interp.push_frame" => {
            kw.only(&["controller", "host", "card_uid", "kind"], f);
            assert_eq!(enum_value(&args[0], "GameState"), "scenario", "{f}");
            let program = ProgramId(small(enum_value(&args[1], "Program")));
            let kind = kw
                .get("kind")
                .and_then(Value::as_str)
                .expect("a frame kind");
            let fa = FrameArgs {
                controller: u8::try_from(kw.int_or("controller", -1)).expect("a controller"),
                host: i32::try_from(kw.int_or("host", -1)).expect("a host"),
                card_uid: i32::try_from(kw.int_or("card_uid", -1)).expect("a card uid"),
                kind: FrameKind::from_value(kind).expect("a frame kind"),
                event: EventPayload::default(),
                vars: &[],
            };
            tk::push_frame(sc.state_mut(), program, fa).unwrap_or_else(|e| panic!("{f}: {e}"));
        }
        other => panic!("a Scenario build calls {other}, which this test does not know"),
    }
}

fn scenario_options(call: &Value) -> ScenarioOptions {
    assert_eq!(call["fn"], "Scenario.__init__");
    let kw = Kwargs::of(call);
    kw.only(
        &["active", "turn", "seed", "deck_size"],
        "Scenario.__init__",
    );
    let d = ScenarioOptions::default();
    ScenarioOptions {
        active: kw.get("active").map_or(d.active, small),
        turn: kw.get("turn").map_or(d.turn, small),
        seed: kw.get("seed").map_or(d.seed, u64_of),
        deck_size: kw
            .get("deck_size")
            .map_or(d.deck_size, |v| usize::try_from(int(v)).expect("a size")),
    }
}

#[derive(Default, Debug)]
struct Counts {
    rebuilt: usize,
    from_snapshot: usize,
    new_game: usize,
    push_frame_builds: usize,
    override_builds: usize,
    force_link_builds: usize,
    tests: usize,
}

/// What the replay of one trace file knows besides the records.
struct Ctx<'a> {
    base: &'a Arc<Registry>,
    reg: Arc<Registry>,
    force_link: Vec<String>,
}

fn check_scenario(ctx: &Ctx<'_>, test: &str, rec: &Value, counts: &mut Counts) {
    let after16 = rec["after16"].as_str().expect("after16");
    let rules_checked = int(&rec["rules_checked"]);
    if let Some(ng) = rec.get("new_game").filter(|v| !v.is_null()) {
        let deck = |p: usize, k: &str| strings(ng["decks"][p][k].as_array().expect("cards"));
        let (m0, r0, m1, r1) = (
            deck(0, "main"),
            deck(0, "res"),
            deck(1, "main"),
            deck(1, "res"),
        );
        let o = NewGameOptions {
            chooser: ng["chooser"]
                .as_u64()
                .map(|c| u8::try_from(c).expect("a player")),
            turn_limit: small(&ng["turn_limit"]),
            max_actions: int(&ng["max_actions"]),
            validate: ng["validate"].as_bool().expect("validate"),
        };
        let st = new_game_by_number(&ctx.reg, [(&m0, &r0), (&m1, &r1)], u64_of(&ng["seed"]), o)
            .unwrap_or_else(|e| panic!("{test}: new_game: {e}"));
        assert_eq!(digest16(&st), after16, "{test}: new_game after16");
        assert_eq!(st.rules_checked(), rules_checked, "{test}: rules_checked");
        counts.new_game += 1;
        return;
    }
    let calls = rec["calls"].as_array().expect("calls");
    let mut sc = Scenario::new(&ctx.reg, scenario_options(&calls[0]));
    if !ctx.force_link.is_empty() {
        let units: Vec<&str> = ctx.force_link.iter().map(String::as_str).collect();
        sc.force_link(&units);
        counts.force_link_builds += 1;
    }
    let mut started = false;
    for c in &calls[1..] {
        assert!(!started, "{test}: a call after Scenario.start");
        assert!(c.get("error").is_none_or(Value::is_null), "{test}: {c}");
        scenario_call(&mut sc, c);
        started = c["fn"] == "Scenario.start";
        if c["fn"] == "interp.push_frame" {
            counts.push_frame_builds += 1;
        }
    }
    assert!(started, "{test}: the build ends with Scenario.start");
    if !Arc::ptr_eq(&ctx.reg, ctx.base) {
        counts.override_builds += 1;
    }
    let before16 = rec["before16"].as_str().expect("before16");
    let mut st = if rec["calls_reproduce"].as_bool().expect("calls_reproduce") {
        let st = sc.into_state();
        assert_eq!(digest16(&st), before16, "{test}: the state before advance");
        assert_eq!(st.rules_checked(), rules_checked, "{test}: rules_checked");
        counts.rebuilt += 1;
        st
    } else {
        let snap = StateSnapshot {
            state: rec["state"].clone(),
            continuous: rec["continuous"].clone(),
            filter_sets: rec["filter_sets"].clone(),
            rules_checked,
        };
        let mut st = from_canonical(&ctx.reg, &snap)
            .unwrap_or_else(|e| panic!("{test}: the pre-advance snapshot: {e}"));
        if !ctx.force_link.is_empty() {
            let units: Vec<&str> = ctx.force_link.iter().map(String::as_str).collect();
            gcg_engine::testkit::set_force_link(&mut st, &units);
        }
        assert_eq!(digest16(&st), before16, "{test}: the pre-advance snapshot");
        counts.from_snapshot += 1;
        st
    };
    tk::advance(&mut st).unwrap_or_else(|e| panic!("{test}: advance: {e}"));
    assert_eq!(digest16(&st), after16, "{test}: the state after advance");
}

#[test]
fn every_per_test_trace_scenario_rebuilds_with_the_testkit() {
    let base = Registry::embedded();
    let mut files = Vec::new();
    trace_files(&tests_dir(), &mut files);
    assert!(files.len() >= 40, "{} trace files", files.len());
    let mut counts = Counts::default();
    for file in &files {
        let text = fs::read_to_string(file).expect("a trace file");
        let mut ctx = Ctx {
            base: &base,
            reg: Arc::clone(&base),
            force_link: Vec::new(),
        };
        let mut test = String::new();
        for line in text.lines() {
            let rec: Value = serde_json::from_str(line).expect("a JSON record");
            match rec["t"].as_str().expect("a record type") {
                "header" => {
                    ctx.reg = Arc::clone(&base);
                    ctx.force_link.clear();
                    rec["id"].as_str().expect("a test id").clone_into(&mut test);
                    counts.tests += 1;
                }
                "override" => {
                    let reg = base
                        .with_fragment(&rec["fragment"])
                        .unwrap_or_else(|e| panic!("{test}: the override fragment: {e}"));
                    ctx.reg = Arc::new(reg);
                }
                "force_link" => {
                    ctx.force_link = strings(rec["units"].as_array().expect("units"))
                        .into_iter()
                        .map(str::to_owned)
                        .collect();
                }
                "scenario" => check_scenario(&ctx, &test, &rec, &mut counts),
                _ => {}
            }
        }
    }
    println!("{counts:?}");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(tests_dir().join("MANIFEST-tests.json")).expect("the manifest"),
    )
    .expect("JSON");
    let traces: i64 = manifest["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| int(&f["traces"]))
        .sum();
    assert_eq!(
        i64::try_from(counts.tests).unwrap(),
        traces,
        "every trace of the manifest was read"
    );
    assert!(
        counts.rebuilt >= 3000 && counts.new_game >= 3000,
        "{counts:?}"
    );
    assert!(
        counts.push_frame_builds >= 4
            && counts.override_builds >= 1
            && counts.force_link_builds >= 4
            && counts.from_snapshot >= 1,
        "{counts:?}"
    );
}
