//! The statistics, seeding and agent-spec vectors of `oracle/export_stats.py`
//! (`tests/vectors/stats.json`): `_rate`, `mirrored_specs`, `comparison_specs` and its errors,
//! `summarize`, the `compare` CLI's exact output bytes over synthetic games, `parse_agent`, the
//! override converters and the Newcombe interval (T-079 formatting, T-080 vectors).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Once;

use gcg_ai::{DEFAULT_WEIGHTS, FeatureSet, Weights};
use gcg_ai_reference::ReferenceConfig;
use gcg_core::SplitMix64;
use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_data::DeckList;
use gcg_eval::agent_spec::parse_agent;
use gcg_eval::bench_cmd::CommandError;
use gcg_eval::cli::{Command, parse};
use gcg_eval::compare::indented;
use gcg_eval::decks::{dir_decks, embedded};
use gcg_eval::pyconv::{py_flag, py_float, py_int};
use gcg_eval::specs::{comparison_specs, mirrored_specs};
use gcg_eval::stats::{newcombe_paired, rate_json, summarize};
use gcg_eval::{AgentDesc, DeckSet, GameOutcome, MirroredSpec};
use gcg_runner::reports::stats::Z95;
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn vectors() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/stats.json");
    let text = std::fs::read_to_string(&path).expect("read stats.json");
    serde_json::from_str(&text).expect("parse stats.json")
}

fn list<'a>(v: &'a Value, key: &str) -> &'a Vec<Value> {
    v[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is a list"))
}

fn u(v: &Value) -> u64 {
    v.as_u64().unwrap_or_else(|| panic!("{v} is a u64"))
}

fn bits(v: &Value) -> u64 {
    u(&v["f64"])
}

fn deck_set(name: &str) -> Vec<DeckList> {
    let decks = |set| {
        embedded(set)
            .expect("embedded set")
            .into_iter()
            .map(|d| d.deck)
            .collect::<Vec<_>>()
    };
    let examples = || {
        dir_decks(&root().join("examples").join("decks"))
            .expect("example decks")
            .into_iter()
            .map(|d| d.deck)
            .collect::<Vec<_>>()
    };
    match name {
        "tests" => decks(DeckSet::Tests),
        "tuning" => decks(DeckSet::Tuning),
        "examples" => examples(),
        "repeated" => {
            let e = examples();
            vec![e[2].clone(), e[2].clone(), e[0].clone()]
        }
        "one" => decks(DeckSet::Tests)[..1].to_vec(),
        "none" => Vec::new(),
        other => panic!("unknown deck set {other}"),
    }
}

fn agents() -> (AgentDesc, AgentDesc) {
    (
        AgentDesc::Random,
        AgentDesc::Greedy {
            weights: DEFAULT_WEIGHTS.clone(),
        },
    )
}

fn check_rows(specs: &[MirroredSpec], rows: &[Value], decks: &[DeckList], what: &str) {
    let (c, _) = agents();
    assert_eq!(specs.len(), rows.len(), "{what}");
    for (s, row) in specs.iter().zip(rows) {
        let agents: String = s
            .agents
            .iter()
            .map(|a| if *a == c { 'c' } else { 'o' })
            .collect();
        let seat_decks: Vec<u64> = s
            .decks
            .iter()
            .map(|d| {
                u64::try_from(decks.iter().position(|x| x == d).expect("a set deck")).unwrap_or(0)
            })
            .collect();
        let got = serde_json::json!([
            s.index,
            s.game_seed,
            s.agent_seeds[0],
            s.agent_seeds[1],
            s.challenger,
            agents,
            seat_decks
        ]);
        assert_eq!(&got, row, "{what}: game {}", s.index);
    }
}

/// The embedded `tests` and `tuning` deck files and the example loader give Python's
/// `DeckList`s, card for card in order.
#[test]
fn deck_sets_are_pythons() {
    let v = vectors();
    let db = gcg_data::CardDb::embedded();
    let numbers = |ids: &[gcg_data::DefId]| -> Vec<String> {
        ids.iter()
            .map(|&id| db.by_id(id).card_number.to_string())
            .collect()
    };
    let sets = v["deck_sets"].as_object().expect("deck_sets");
    assert_eq!(sets.len(), 3);
    for (name, decks) in sets {
        let rust = deck_set(name);
        let python = decks.as_array().expect("decks");
        assert_eq!(rust.len(), python.len(), "{name}");
        for (r, p) in rust.iter().zip(python) {
            let main: Vec<String> = list(p, "main")
                .iter()
                .map(|x| x.as_str().unwrap_or_default().to_owned())
                .collect();
            let resources: Vec<String> = list(p, "resources")
                .iter()
                .map(|x| x.as_str().unwrap_or_default().to_owned())
                .collect();
            assert_eq!(numbers(&r.main), main, "{name} main");
            assert_eq!(numbers(&r.resources), resources, "{name} resources");
        }
    }
}

#[test]
fn rates_match_compare_rate() {
    let v = vectors();
    let plain = DumpOptions {
        indent: None,
        sort_keys: false,
        compact: false,
        ensure_ascii: true,
    };
    for case in list(&v, "rates") {
        let (w, n) = (u(&case[0]), u(&case[1]));
        assert_eq!(
            dumps(&rate_json(w, n), plain),
            case[2].as_str().unwrap_or_default(),
            "_rate({w}, {n})"
        );
    }
}

#[test]
fn mirrored_specs_match_selfplay() {
    let v = vectors();
    let decks = deck_set("tests");
    let (c, o) = agents();
    for case in list(&v, "mirrored_specs") {
        let games = u32::try_from(u(&case["games"])).unwrap_or(0);
        let master = u(&case["master"]);
        let specs = mirrored_specs(games, master, &c, &o, [&decks[0], &decks[1]]);
        check_rows(
            &specs,
            list(case, "rows"),
            &decks,
            &format!("mirrored {games} {master}"),
        );
    }
}

#[test]
fn comparison_specs_match_compare() {
    let v = vectors();
    let (c, o) = agents();
    for case in list(&v, "comparison_specs") {
        let name = case["decks"].as_str().unwrap_or_default();
        let decks = deck_set(name);
        let games = u32::try_from(u(&case["games"])).unwrap_or(0);
        let seed = u(&case["seed"]);
        let specs = comparison_specs(games, seed, &c, &o, &decks).expect("valid specs");
        check_rows(
            &specs,
            list(case, "rows"),
            &decks,
            &format!("{name} {games} {seed}"),
        );
    }
    for case in list(&v, "spec_errors") {
        let name = case["decks"].as_str().unwrap_or_default();
        let games = case["games"].as_i64().unwrap_or(0);
        let games = u32::try_from(games.max(0)).unwrap_or(0);
        let got = comparison_specs(games, 1, &c, &o, &deck_set(name)).map(|_| ());
        assert_eq!(
            got.map_err(|e| e.to_string()),
            Err(case["error"].as_str().unwrap_or_default().to_owned()),
            "{name} {games}"
        );
    }
}

/// `fake_outcome` of `export_stats.py`.
fn fake_outcome(s: &MirroredSpec) -> GameOutcome {
    let mut r = SplitMix64::new(s.game_seed ^ s.agent_seeds[0]);
    let winner = i8::try_from(r.randrange(3)).unwrap_or(0) - 1;
    let turns = 1 + i32::try_from(r.randrange(40)).unwrap_or(0);
    let first = u8::try_from(r.randrange(2)).unwrap_or(0);
    let decisions = u32::try_from(r.randrange(1000)).unwrap_or(0);
    GameOutcome {
        index: s.index,
        challenger: s.challenger,
        winner,
        turns,
        first_player: first,
        decisions,
    }
}

#[test]
fn summarize_matches_compare() {
    let v = vectors();
    let (c, o) = agents();
    for case in list(&v, "summarize") {
        let name = case["decks"].as_str().unwrap_or_default();
        let decks = deck_set(name);
        let games = u32::try_from(u(&case["games"])).unwrap_or(0);
        let specs = comparison_specs(games, u(&case["seed"]), &c, &o, &decks).expect("specs");
        let outcomes: Vec<GameOutcome> = specs.iter().map(fake_outcome).collect();
        let rows: Vec<Value> = outcomes
            .iter()
            .map(|x| {
                serde_json::json!([x.challenger, x.winner, x.turns, x.first_player, x.decisions])
            })
            .collect();
        assert_eq!(&rows, list(case, "outcomes"), "fake outcomes {name}");
        assert_eq!(
            dumps(&summarize(&specs, &outcomes), indented(2)),
            case["text"].as_str().unwrap_or_default(),
            "summarize {name} {games}"
        );
    }
}

static CHDIR: Once = Once::new();

#[test]
fn compare_cli_writes_pythons_bytes() {
    CHDIR.call_once(|| std::env::set_current_dir(root()).expect("cd to the repository root"));
    let v = vectors();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compare-cli");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    for (k, case) in list(&v, "compare_cli").iter().enumerate() {
        let mut argv: Vec<OsString> = vec!["compare".into()];
        argv.extend(
            list(case, "argv")
                .iter()
                .map(|a| OsString::from(a.as_str().unwrap_or_default())),
        );
        let out = dir.join(format!("report-{k}.json"));
        argv.push("--out".into());
        argv.push(out.clone().into_os_string());
        let Ok(Command::Compare(args)) = parse(&argv, 80) else {
            panic!("compare arguments {argv:?}")
        };
        let mut play = |specs: &[MirroredSpec]| -> Result<Vec<GameOutcome>, CommandError> {
            Ok(specs.iter().map(fake_outcome).collect())
        };
        let stdout = gcg_eval::compare::run(&args, &mut play).expect("compare runs");
        assert_eq!(
            stdout,
            case["stdout"].as_str().unwrap_or_default(),
            "stdout {argv:?}"
        );
        let file = std::fs::read_to_string(&out).expect("the --out file");
        assert_eq!(
            file,
            case["out"].as_str().unwrap_or_default(),
            "--out {argv:?}"
        );
    }
}

fn config_matches(cfg: &ReferenceConfig, v: &Value) {
    let f = |k: &str| bits(&v[k]);
    let n = |k: &str| u(&v[k]);
    let b = |k: &str| v[k].as_bool().unwrap_or_else(|| panic!("{k} is a bool"));
    assert_eq!(cfg.name, v["name"].as_str().unwrap_or_default());
    assert_eq!(u64::from(cfg.max_iterations), n("max_iterations"));
    assert_eq!(u64::from(cfg.min_iterations), n("min_iterations"));
    assert_eq!(
        u64::from(cfg.iterations_per_option),
        n("iterations_per_option")
    );
    assert_eq!(cfg.exploration.to_bits(), f("exploration"));
    assert_eq!(cfg.prior_weight.to_bits(), f("prior_weight"));
    assert_eq!(cfg.prior_temperature.to_bits(), f("prior_temperature"));
    assert_eq!(
        i64::from(cfg.horizon_turns),
        v["horizon_turns"].as_i64().unwrap_or(i64::MIN)
    );
    assert_eq!(cfg.rollout_temperature.to_bits(), f("rollout_temperature"));
    assert_eq!(cfg.value_scale.to_bits(), f("value_scale"));
    assert_eq!(
        u64::from(cfg.max_rollout_decisions),
        n("max_rollout_decisions")
    );
    assert_eq!(u64::from(cfg.first_player_games), n("first_player_games"));
    assert_eq!(u64::from(cfg.mulligan_samples), n("mulligan_samples"));
    assert_eq!(
        cfg.policy_burst_yes_score.to_bits(),
        f("policy_burst_yes_score")
    );
    assert_eq!(cfg.noop_branch_choices, b("noop_branch_choices"));
    assert_eq!(cfg.noop_resource_aware, b("noop_resource_aware"));
    assert_eq!(cfg.strict_attack_prune, b("strict_attack_prune"));
    assert_eq!(cfg.prune_noop_rest_targets, b("prune_noop_rest_targets"));
    assert_eq!(cfg.merge_equivalent_moves, b("merge_equivalent_moves"));
    assert_eq!(cfg.observation_keys, b("observation_keys"));
    assert_eq!(cfg.target_extension.to_bits(), f("target_extension"));
    assert_eq!(cfg.target_extension_z.to_bits(), f("target_extension_z"));
    assert_eq!(
        cfg.policy_rule_polarity_table,
        b("policy_rule_polarity_table")
    );
    assert_eq!(cfg.policy_zero_ap_face, b("policy_zero_ap_face"));
    assert_eq!(
        v.as_object().map(serde_json::Map::len),
        Some(gcg_eval::agent_spec::SEARCH_FIELDS.len())
    );
}

fn weights_of(v: &Value) -> Weights {
    if v.is_null() {
        return DEFAULT_WEIGHTS.clone();
    }
    let set =
        FeatureSet::from_value(v["feature_set"].as_str().unwrap_or_default()).expect("feature set");
    let values = list(v, "values")
        .iter()
        .map(|x| f64::from_bits(bits(x)))
        .collect();
    Weights::new(values, set).expect("weights")
}

#[test]
fn parse_agent_matches_python() {
    let v = vectors();
    for case in list(&v, "parse_agent") {
        let text = case["text"].as_str().unwrap_or_default();
        let got = parse_agent(text);
        if let Some(error) = case.get("error") {
            assert_eq!(got.err().as_deref(), error.as_str(), "{text:?}");
            continue;
        }
        let desc = got.unwrap_or_else(|e| panic!("{text:?} failed: {e}"));
        assert_eq!(case["preset"], "standard");
        match (case["kind"].as_str().unwrap_or_default(), &desc) {
            ("random", AgentDesc::Random) => {
                assert!(case["config"].is_null() && case["weights"].is_null());
            }
            ("greedy", AgentDesc::Greedy { weights }) => {
                assert!(case["config"].is_null());
                assert_eq!(weights, &weights_of(&case["weights"]), "{text:?}");
            }
            ("mcts", AgentDesc::Reference { config, weights }) => {
                config_matches(config, &case["config"]);
                assert_eq!(weights, &weights_of(&case["weights"]), "{text:?}");
            }
            (kind, other) => panic!("{text:?}: Python {kind}, Rust {other:?}"),
        }
    }
}

#[test]
fn converters_match_python() {
    let v = vectors();
    for case in list(&v, "int") {
        let text = case["text"].as_str().unwrap_or_default();
        let got = py_int(text).map(|x| x.to_decimal());
        match case.get("error") {
            Some(e) => assert_eq!(got.err().as_deref(), e.as_str(), "int({text:?})"),
            None => assert_eq!(got.ok().as_deref(), case["value"].as_str(), "int({text:?})"),
        }
    }
    for case in list(&v, "float") {
        let text = case["text"].as_str().unwrap_or_default();
        let got = py_float(text).map(f64::to_bits);
        match case.get("error") {
            Some(e) => assert_eq!(got.err().as_deref(), e.as_str(), "float({text:?})"),
            None => assert_eq!(got.ok(), Some(bits(&case["value"])), "float({text:?})"),
        }
    }
    for case in list(&v, "flag") {
        let text = case["text"].as_str().unwrap_or_default();
        let got = py_flag(text);
        match case.get("error") {
            Some(e) => assert_eq!(got.err().as_deref(), e.as_str(), "_flag({text:?})"),
            None => assert_eq!(got.ok(), case["value"].as_bool(), "_flag({text:?})"),
        }
    }
}

#[test]
fn newcombe_matches_the_python_reference() {
    let v = vectors();
    for case in list(&v, "newcombe") {
        let t: Vec<u64> = list(case, "table").iter().map(u).collect();
        let (lo, hi) = newcombe_paired(t[0], t[1], t[2], t[3], Z95);
        assert_eq!(
            (lo.to_bits(), hi.to_bits()),
            (bits(&case["lo"]), bits(&case["hi"])),
            "{t:?}: {lo} {hi}"
        );
    }
}
