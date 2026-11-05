//! `--ai-preset` takes exactly `standard` (the default) and `fast` (PLAN §6.5
//! item 3); the typed commands of the parsed namespace; and `main`'s flow from
//! arguments to exit codes 0/1/2/130 (`cli.py:386-399`).

use std::ffi::OsString;
use std::io::Write;

use gcg_cli::cli::{
    self, BenchmarkArgs, Command, CommandError, Commands, DataStatusArgs, PathArg, ReplayArgs,
    ValidateArgs,
};
use gcg_runner::argparse::{ActionKind, Parser, PyStr};
use gcg_runner::{Format, Workers};
use gcg_search::Preset;

const D1: &str = "examples/decks/blue-white-federation.txt";
const D2: &str = "examples/decks/red-green-zeon.txt";

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn bench(extra: &[&str]) -> Vec<OsString> {
    let mut args = vec!["benchmark", D1, D2, "--matches", "2"];
    args.extend_from_slice(extra);
    os(&args)
}

fn usage_error(args: &[OsString]) -> String {
    match cli::parse(args, 80) {
        Ok(command) => panic!("parsed: {command:?}"),
        Err(exit) => {
            assert_eq!(exit.status, 2);
            assert!(exit.stdout.is_empty());
            String::from_utf8(exit.stderr).expect("UTF-8")
        }
    }
}

fn last_line(text: &str) -> &str {
    text.lines().last().unwrap_or("")
}

fn benchmark(args: &[OsString]) -> BenchmarkArgs {
    match cli::parse(args, 80) {
        Ok(Command::Benchmark(b)) => b,
        other => panic!("not a benchmark: {other:?}"),
    }
}

#[test]
fn only_standard_and_fast_are_accepted() {
    for bad in [
        "strong",
        "Standard",
        "",
        "reference",
        "mcts-standard",
        "fast ",
        "weak",
    ] {
        let err = usage_error(&bench(&["--ai-preset", bad]));
        let repr = PyStr::from(bad).repr().to_string_opt().expect("ASCII");
        let line = last_line(&err);
        assert!(
            line.starts_with(&format!(
                "gcg-sim benchmark: error: argument --ai-preset: invalid choice: {repr}"
            )),
            "{line}"
        );
        assert!(line.ends_with("(choose from 'standard', 'fast')"), "{line}");
    }
    assert_eq!(
        last_line(&usage_error(&bench(&["--ai-preset", "strong"]))),
        "gcg-sim benchmark: error: argument --ai-preset: invalid choice: 'strong' (choose from \
         'standard', 'fast')"
    );
    assert_eq!(benchmark(&bench(&[])).ai_preset, Preset::Standard);
    assert_eq!(
        benchmark(&bench(&["--ai-preset", "fast"])).ai_preset,
        Preset::Fast
    );
    assert_eq!(
        benchmark(&bench(&["--ai", "standard"])).ai_preset,
        Preset::Standard
    );
}

#[test]
fn the_help_uses_the_brief_wording_and_never_names_the_reference_agents() {
    let help = String::from_utf8(
        cli::parse(&os(&["benchmark", "--help"]), 1000)
            .expect_err("help")
            .stdout,
    )
    .expect("UTF-8");
    for preset in Preset::ALL {
        assert!(help.contains(preset.help()), "{}", preset.as_str());
    }
    assert!(help.contains("--ai-preset {standard,fast}"));
    assert!(help.contains("auto"));
    assert!(!help.contains("mcts-"));
    assert!(!help.contains("strong}"));
}

fn walk(parser: &Parser, out: &mut Vec<String>) {
    for action in &parser.actions {
        out.extend(action.help.clone());
        if let ActionKind::SubParsers(commands) = &action.kind {
            for command in commands {
                out.extend(command.help.map(str::to_owned));
                walk(&command.parser, out);
            }
        }
    }
    out.extend(parser.description.map(str::to_owned));
}

#[test]
fn help_strings_are_ascii_without_format_specifiers() {
    // The Rust textwrap uses ASCII classes, and argparse would %-format a '%'.
    let mut texts = Vec::new();
    walk(&cli::build_parser(), &mut texts);
    assert!(texts.len() >= 20);
    for text in texts {
        assert!(text.is_ascii(), "{text}");
        assert!(!text.contains('%'), "{text}");
    }
}

#[test]
fn a_full_benchmark_command_line() {
    let args = os(&[
        "benchmark",
        "--matches",
        "12",
        "./decks//a.txt",
        "--seed",
        "0xffffffffffffffff",
        "b.txt",
        "--format",
        "bo1",
        "--out",
        "./out/",
        "--decision-log",
        "--replays",
        "0",
        "--workers",
        "auto",
    ]);
    assert_eq!(
        benchmark(&args),
        BenchmarkArgs {
            deck_under_test: PathArg(PyStr::from("decks/a.txt")),
            benchmark_deck: PathArg(PyStr::from("b.txt")),
            matches: 12,
            seed: u64::MAX,
            workers: Workers::Auto,
            format: Format::Bo1,
            ai_preset: Preset::Standard,
            out: PathArg(PyStr::from("out")),
            decision_log: true,
            replays: 0,
        }
    );
    let defaults = benchmark(&bench(&[]));
    assert_eq!(defaults.seed, 0);
    assert_eq!(defaults.workers, Workers::Count(1));
    assert_eq!(defaults.format, Format::Bo3);
    assert_eq!(defaults.out, PathArg(PyStr::from("gcg-sim-out")));
    assert!(!defaults.decision_log);
    assert_eq!(defaults.replays, 3);
}

#[test]
fn integers_beyond_the_config_types() {
    // Python caps workers at the match count, so any larger count behaves as u32::MAX.
    assert_eq!(
        benchmark(&bench(&["--workers", "99999999999999999999"])).workers,
        Workers::Count(u32::MAX)
    );
    assert_eq!(
        benchmark(&os(&["benchmark", D1, D2, "--matches", "4294967295"])).matches,
        u32::MAX
    );
    // Documented difference: BenchmarkConfig holds u32 counts; Python would accept these.
    assert_eq!(
        last_line(&usage_error(&os(&[
            "benchmark",
            D1,
            D2,
            "--matches",
            "4294967296"
        ]))),
        "gcg-sim benchmark: error: argument --matches: must be at most 4294967295, got 4294967296"
    );
    assert_eq!(
        last_line(&usage_error(&bench(&["--replays", "99999999999"]))),
        "gcg-sim benchmark: error: argument --replays: must be at most 4294967295, got 99999999999"
    );
    let huge = format!("0x{}", "f".repeat(4000));
    assert_eq!(
        last_line(&usage_error(&os(&[
            "benchmark",
            D1,
            D2,
            "--matches",
            &huge
        ]))),
        "gcg-sim benchmark: error: argument --matches: must be at most 4294967295"
    );
}

#[test]
fn the_other_commands() {
    assert_eq!(
        cli::parse(&os(&["validate", "--", "--weird"]), 80),
        Ok(Command::Validate(ValidateArgs {
            deck: PathArg(PyStr::from("--weird"))
        }))
    );
    assert_eq!(
        cli::parse(&os(&["replay", "--lo", "r.json"]), 80),
        Ok(Command::Replay(ReplayArgs {
            replay_json: PathArg(PyStr::from("r.json")),
            log: true
        }))
    );
    assert_eq!(
        cli::parse(&os(&["data", "status", "--j"]), 80),
        Ok(Command::DataStatus(DataStatusArgs { json: true }))
    );
    assert_eq!(
        cli::parse(&os(&["data", "status"]), 80),
        Ok(Command::DataStatus(DataStatusArgs { json: false }))
    );
}

#[test]
fn terminal_columns_follow_shutil() {
    let cols = |v: Option<&str>| cli::terminal_columns(v.map(std::ffi::OsStr::new));
    assert_eq!(cols(None), 80);
    for bad in ["abc", "0", "-5", "", "1.5"] {
        assert_eq!(cols(Some(bad)), 80, "{bad}");
    }
    assert_eq!(cols(Some(" 60 ")), 60);
    assert_eq!(cols(Some("6_0")), 60);
    assert_eq!(cols(Some("٦٠")), 60);
    assert_eq!(cols(Some("99999999999999999999999")), i64::MAX);
}

/// A recording stand-in for the command bodies.
#[derive(Default)]
struct Recorder {
    calls: Vec<Command>,
    fail: Option<CommandError>,
}

impl Recorder {
    fn finish(&mut self, command: Command, out: &mut dyn Write) -> Result<(), CommandError> {
        if let Command::Validate(v) = &command {
            out.write_all(&v.deck.stdout_bytes()).expect("write");
            out.write_all(b": checked\n").expect("write");
        }
        self.calls.push(command);
        self.fail.clone().map_or(Ok(()), Err)
    }
}

impl Commands for Recorder {
    fn benchmark(
        &mut self,
        args: &BenchmarkArgs,
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> Result<(), CommandError> {
        err.write_all(b"matches 1/1\n").expect("write");
        self.finish(Command::Benchmark(args.clone()), out)
    }

    fn validate(&mut self, args: &ValidateArgs, out: &mut dyn Write) -> Result<(), CommandError> {
        self.finish(Command::Validate(args.clone()), out)
    }

    fn data_status(
        &mut self,
        args: &DataStatusArgs,
        out: &mut dyn Write,
    ) -> Result<(), CommandError> {
        self.finish(Command::DataStatus(*args), out)
    }

    fn replay(&mut self, args: &ReplayArgs, out: &mut dyn Write) -> Result<(), CommandError> {
        self.finish(Command::Replay(args.clone()), out)
    }
}

fn run(args: &[OsString], fail: Option<CommandError>) -> (u8, Vec<u8>, Vec<u8>, Vec<Command>) {
    let mut rec = Recorder {
        calls: Vec::new(),
        fail,
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = cli::run(args, None, &mut rec, &mut out, &mut err);
    (status, out, err, rec.calls)
}

#[test]
fn main_maps_outcomes_to_exit_codes() {
    use std::os::unix::ffi::OsStringExt as _;

    let (status, out, err, calls) = run(&os(&["--help"]), None);
    assert_eq!((status, err.is_empty(), calls.len()), (0, true, 0));
    assert!(out.starts_with(b"usage: gcg-sim [-h] {benchmark,validate,data,replay} ..."));

    let (status, out, err, calls) = run(&os(&[]), None);
    assert_eq!((status, out.is_empty(), calls.len()), (2, true, 0));
    assert!(err.ends_with(b"gcg-sim: error: the following arguments are required: command\n"));

    let deck = OsString::from_vec(b"d\xff.txt".to_vec());
    let (status, out, err, calls) = run(&[OsString::from("validate"), deck], None);
    assert_eq!((status, err.is_empty()), (0, true));
    assert_eq!(out, b"d\xff.txt: checked\n");
    assert_eq!(calls.len(), 1);

    let (status, out, err, _) = run(
        &os(&["data", "status"]),
        Some(CommandError::Invalid(PyStr::from_code_points(vec![
            0x62, 0xDCFF,
        ]))),
    );
    assert_eq!((status, out.is_empty()), (1, true));
    assert_eq!(err, b"error: b\\udcff\n");

    let (status, out, err, calls) = run(&bench(&[]), Some(CommandError::Interrupted));
    assert_eq!((status, out.is_empty()), (130, true));
    assert_eq!(
        err,
        b"matches 1/1\ninterrupted: workers stopped, no reports written\n"
    );
    assert!(matches!(calls.as_slice(), [Command::Benchmark(_)]));
}
