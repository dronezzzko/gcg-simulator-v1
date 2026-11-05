//! The `gcg-eval` command line: subcommands, defaults, the reference-benchmark and
//! preset-benchmark flags (every `gcg-sim benchmark` flag plus their own), head-to-head's
//! selections and seed roles, compare's `type=int` options, and the usage errors (exit 2).

use std::ffi::OsString;
use std::path::PathBuf;

use gcg_core::pool::Cancel;
use gcg_eval::cli::{Command, parse, run};
use gcg_eval::head_to_head::PairSource;
use gcg_eval::{ChooseFirst, DeckSet, PairSet, SeedRole};
use gcg_runner::{Format, TestAgentKind, Workers};

fn argv(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn parsed(args: &[&str]) -> Command {
    parse(&argv(args), 80).unwrap_or_else(|e| {
        panic!(
            "{args:?}: exit {} {}",
            e.status,
            String::from_utf8_lossy(&e.stderr)
        )
    })
}

/// `(exit status, stdout, stderr)` of `main`, for arguments that stop before any game.
fn exit(args: &[&str]) -> (u8, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = run(&argv(args), None, &Cancel::new(), false, &mut out, &mut err);
    (
        status,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

#[test]
fn help_lists_the_subcommands() {
    let (status, out, _) = exit(&["--help"]);
    assert_eq!(status, 0);
    for name in [
        "reference-benchmark",
        "preset-benchmark",
        "head-to-head",
        "compare",
        "env",
    ] {
        assert!(out.contains(name), "{name} missing from {out}");
    }
    let (status, out, _) = exit(&["reference-benchmark", "--help"]);
    assert_eq!(status, 0);
    for flag in [
        "--matches",
        "--seed",
        "--workers",
        "--format",
        "--reference-preset",
        "--out",
        "--decision-log",
        "--replays",
        "--agent",
    ] {
        assert!(out.contains(flag), "{flag} missing from {out}");
    }
    assert!(!out.contains("[--ai-preset"), "{out}");
}

#[test]
fn reference_benchmark_defaults_and_flags() {
    let Command::ReferenceBenchmark { args, agent } =
        parsed(&["reference-benchmark", "a.txt", "b.txt", "--matches", "3"])
    else {
        panic!("reference-benchmark")
    };
    assert_eq!(agent, None);
    assert_eq!(
        (args.preset, args.seed, args.workers, args.format),
        ("standard", 0, Workers::Count(1), Format::Bo3)
    );
    assert_eq!(
        (
            args.out.as_str(),
            args.replays,
            args.decision_log,
            args.matches
        ),
        ("gcg-sim-out", 3, false, 3)
    );
    let Command::ReferenceBenchmark { args, agent } = parsed(&[
        "reference-benchmark",
        "./a.txt",
        "b.txt",
        "--matches",
        "0x10",
        "--reference-preset",
        "strong",
        "--agent",
        "test-random",
        "--workers",
        "auto",
        "--format",
        "bo1",
        "--decision-log",
        "--seed",
        "18446744073709551615",
        "--out",
        "out//dir/",
        "--replays",
        "0",
    ]) else {
        panic!("reference-benchmark")
    };
    assert_eq!(agent, Some(TestAgentKind::Random));
    assert_eq!(args.deck_under_test, "a.txt");
    assert_eq!(args.out, "out/dir");
    assert_eq!(
        (
            args.preset,
            args.seed,
            args.workers,
            args.format,
            args.matches
        ),
        ("strong", u64::MAX, Workers::Auto, Format::Bo1, 16)
    );
    assert!(args.decision_log && args.replays == 0);
}

#[test]
fn usage_errors_exit_2() {
    for (args, needle) in [
        (
            &[
                "reference-benchmark",
                "a",
                "b",
                "--matches",
                "1",
                "--reference-preset",
                "huge",
            ][..],
            "invalid choice",
        ),
        (
            &[
                "reference-benchmark",
                "a",
                "b",
                "--matches",
                "1",
                "--ai-preset",
                "fast",
            ][..],
            "unrecognized arguments",
        ),
        (&["reference-benchmark", "a", "b"][..], "--matches"),
        (
            &["reference-benchmark", "a", "b", "--matches", "4294967296"][..],
            "must be at most 4294967295",
        ),
        (
            &["preset-benchmark", "a", "b", "--matches", "1"][..],
            "--search-config",
        ),
        (
            &[
                "preset-benchmark",
                "a",
                "b",
                "--matches",
                "1",
                "--search-config",
                "c.json",
                "--ai-preset",
                "strong",
            ][..],
            "invalid choice",
        ),
        (&["head-to-head", "random"][..], "opponent"),
        (
            &[
                "head-to-head",
                "random",
                "greedy",
                "--choose-first",
                "seat1",
            ][..],
            "invalid choice",
        ),
        (
            &["head-to-head", "random", "greedy", "--role", "evaluation"][..],
            "invalid choice",
        ),
        (
            &["compare", "random", "random", "--games", "x"][..],
            "argument --games: invalid int value: 'x'",
        ),
        (
            &["compare", "random", "random", "--workers", "1.5"][..],
            "argument --workers: invalid int value: '1.5'",
        ),
        // Python converts `type=int` while parsing, so a bad --games is reported before a
        // missing positional.
        (
            &["compare", "random", "--games", "x"][..],
            "argument --games: invalid int value: 'x'",
        ),
        (
            &[
                "compare",
                "random",
                "random",
                "--seed",
                "1000000000000000000000000000000000000000",
            ][..],
            "argument --seed: 1000000000000000000000000000000000000000 is outside the range gcg-eval supports",
        ),
        (&["nonsense"][..], "invalid choice"),
    ] {
        let (status, out, err) = exit(args);
        assert_eq!(status, 2, "{args:?}: {err}");
        assert!(out.is_empty(), "{args:?}");
        assert!(err.contains(needle), "{args:?}: {err}");
        assert!(err.starts_with("usage: gcg-eval"), "{args:?}: {err}");
    }
}

#[test]
fn preset_benchmark_takes_a_search_config() {
    let Command::PresetBenchmark {
        args,
        search_config,
    } = parsed(&[
        "preset-benchmark",
        "a.txt",
        "b.txt",
        "--matches",
        "20",
        "--search-config",
        "cand.json",
        "--ai-preset",
        "fast",
        "--workers",
        "10",
    ])
    else {
        panic!("preset-benchmark")
    };
    assert_eq!(search_config, PathBuf::from("cand.json"));
    assert_eq!(
        (args.preset, args.workers, args.matches),
        ("fast", Workers::Count(10), 20)
    );
}

#[test]
fn head_to_head_selections() {
    let Command::HeadToHead(a) = parsed(&["head-to-head", "search:standard", "ref:strong"]) else {
        panic!("head-to-head")
    };
    assert_eq!(
        a.source,
        PairSource::Set {
            decks: DeckSet::Tests,
            pairs: PairSet::All
        }
    );
    assert_eq!(
        (a.games, a.seed, a.role, a.workers, a.choose_first),
        (400, 1, None, Workers::Count(1), ChooseFirst::Seat0)
    );
    assert_eq!(a.ledger, PathBuf::from("docs/dev/eval/seed-ledger.json"));
    let Command::HeadToHead(a) = parsed(&[
        "head-to-head",
        "random",
        "greedy:weights=hand",
        "--decks",
        "panel",
        "--pairs",
        "held-out",
        "--games",
        "11200",
        "--seed",
        "90000001",
        "--role",
        "eval",
        "--workers",
        "auto",
        "--choose-first",
        "agents",
        "--out",
        "e1.json",
    ]) else {
        panic!("head-to-head")
    };
    assert_eq!(
        a.source,
        PairSource::Set {
            decks: DeckSet::Panel,
            pairs: PairSet::HeldOut
        }
    );
    assert_eq!(
        (a.games, a.seed, a.role, a.workers, a.choose_first),
        (
            11200,
            90_000_001,
            Some(SeedRole::Eval),
            Workers::Auto,
            ChooseFirst::Agents
        )
    );
    assert_eq!(a.out, Some(PathBuf::from("e1.json")));
    let Command::HeadToHead(a) = parsed(&[
        "head-to-head",
        "random",
        "random",
        "--pair",
        "tuning-zeon:tuning-wing,examples/decks/red-green-zeon.txt:tests-seed",
    ]) else {
        panic!("head-to-head")
    };
    assert_eq!(
        a.source,
        PairSource::Named(
            "tuning-zeon:tuning-wing,examples/decks/red-green-zeon.txt:tests-seed".to_owned()
        )
    );
    // `--pair` is repeatable: the occurrences join in order.
    let Command::HeadToHead(b) = parsed(&[
        "head-to-head",
        "random",
        "random",
        "--pair",
        "tuning-zeon:tuning-wing",
        "--pair",
        "examples/decks/red-green-zeon.txt:tests-seed",
    ]) else {
        panic!("head-to-head")
    };
    assert_eq!(b.source, a.source);
}

#[test]
fn compare_keeps_pythons_int_arguments() {
    let Command::Compare(a) = parsed(&["compare", "standard", "baseline-standard"]) else {
        panic!("compare")
    };
    assert_eq!(
        (a.games, a.seed, a.workers, a.decks.as_str(), a.out),
        (400, 1, 1, "tests", None)
    );
    let Command::Compare(a) = parsed(&[
        "compare",
        "random",
        "greedy",
        "--games",
        " 1_2 ",
        "--seed",
        "-3",
        "--workers",
        "0",
        "--decks",
        "tuning",
        "--out",
        "r.json",
    ]) else {
        panic!("compare")
    };
    assert_eq!((a.games, a.seed, a.workers), (12, -3, 0));
    assert_eq!(a.workers(), Workers::Count(1));
    assert_eq!(gcg_eval::compare::mask64(a.seed), u64::MAX - 2);
}

#[test]
fn env_and_role_failures() {
    assert_eq!(parsed(&["env"]), Command::Env);
    let (status, out, err) = exit(&["env"]);
    assert_eq!((status, err.as_str()), (0, ""));
    let doc: serde_json::Value = serde_json::from_str(&out).expect("env prints JSON");
    assert_eq!(
        doc["code_digest"].as_str().map(str::len),
        Some(64),
        "the comparison script greps code_digest"
    );
    assert!(
        doc["topology"]["logical_cpus"]
            .as_u64()
            .is_some_and(|n| n >= 1)
    );
    let (status, _, err) = exit(&[
        "head-to-head",
        "random",
        "random",
        "--role",
        "tuning",
        "--seed",
        "1",
    ]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "error: --seed 1 is outside the tuning range [10000000, 19999999] (PLAN A14)\n"
    );
    let (status, _, err) = exit(&["head-to-head", "random", "mcts"]);
    assert_eq!(status, 1);
    assert!(
        err.starts_with("error: unknown agent 'mcts'; choose from"),
        "{err}"
    );
    let (status, _, err) = exit(&[
        "head-to-head",
        "random",
        "random",
        "--decks",
        "tuning",
        "--games",
        "8",
    ]);
    assert_eq!(
        (status, err.as_str()),
        (1, "error: games must be a positive multiple of 12\n")
    );
    let (status, _, err) = exit(&[
        "compare",
        "random",
        "random",
        "--decks",
        "tests.ai.decks:DECKS",
    ]);
    assert_eq!(status, 1);
    assert!(err.contains("names a Python module"), "{err}");
}

/// `cmd_benchmark`'s checks before any game (`cli.py:119-126`, `:176-185`), with the texts
/// Python prints for the same inputs (checked against `gcg-sim benchmark` by hand).
#[test]
fn benchmark_checks_decks_and_the_report_directory_first() {
    let zeon = "../../examples/decks/red-green-zeon.txt";
    let federation = "../../examples/decks/blue-white-federation.txt";
    let (status, out, err) = exit(&[
        "reference-benchmark",
        zeon,
        "missing-deck.txt",
        "--matches",
        "1",
    ]);
    assert_eq!((status, out.as_str()), (1, ""));
    assert_eq!(
        err,
        "error: cannot read deck file missing-deck.txt: No such file or directory\n"
    );
    let (status, _, err) = exit(&[
        "reference-benchmark",
        zeon,
        federation,
        "--matches",
        "1",
        "--out",
        "Cargo.toml/sub",
    ]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "error: cannot write reports to Cargo.toml/sub: Cargo.toml is not a directory\n"
    );
    let (status, _, err) = exit(&[
        "preset-benchmark",
        zeon,
        federation,
        "--matches",
        "1",
        "--search-config",
        "no-such-config.json",
    ]);
    assert_eq!(status, 1);
    assert!(
        err.starts_with("error: cannot read search configuration no-such-config.json: "),
        "{err}"
    );
}

/// `--pairs tuning|held-out` need the pre-registered split (PLAN §12.5); without it they fail
/// before any game instead of treating every pair as a tuning pair.
#[test]
fn held_out_selections_need_the_panel_split() {
    for pairs in ["tuning", "held-out"] {
        let (status, _, err) = exit(&[
            "head-to-head",
            "random",
            "random",
            "--decks",
            "tuning",
            "--pairs",
            pairs,
        ]);
        assert_eq!(status, 1);
        assert_eq!(
            err,
            format!(
                "error: --pairs {pairs} needs the held-out split in docs/dev/eval/panel.json\n"
            )
        );
    }
}
