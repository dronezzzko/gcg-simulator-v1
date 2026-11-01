//! The `gcg-diff` command line (PLAN §8.7, §9.9-§9.10, §12.2). Exit codes: 0 success, 1 a
//! check failed, 2 usage.
//!
//! Commands that need the engine, the agents or the runner (`explain`, `trace-search`, `fuzz`,
//! `paranoid-games`, `coverage`, `traceability`, `schema-check`, and `corpus` without
//! `--parse-only`/`--mock`) are not part of this build: they join with the workstreams that
//! provide what they run (C/B2, D1/D2, E), and asking for one is a usage error that says so.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::corpus::{self, CorpusArgs, EngineKind};
use crate::manifest::Repo;

const USAGE: &str = "\
usage: gcg-diff <command> [options]

commands:
  corpus [--tier 1|2] [--only SET[:FILTER] ...] [--file TRACE.ndjson] (--parse-only | --mock)
         [--roundtrip] [--require-all] [--jobs N] [--report FILE] [--junit FILE]
  selftest
  lints
  tests-json --check
  weaknesses --check FILE [--require-resolved]
";

const PENDING: [(&str, &str); 7] = [
    (
        "explain",
        "the engine's from_canonical and canonical state (workstream C)",
    ),
    (
        "trace-search",
        "the reference search with a tracer (workstream D2)",
    ),
    ("fuzz", "the engine (workstreams C, B2)"),
    (
        "paranoid-games",
        "the engine's paranoid build (workstreams C, B2)",
    ),
    ("coverage", "the engine replay census (workstreams C, B2)"),
    (
        "traceability",
        "the per-test replay JUnit (engine) and export_tags.py (G2)",
    ),
    (
        "schema-check",
        "the results schema validator (workstream E)",
    ),
];

fn usage(msg: &str) -> ExitCode {
    eprintln!("gcg-diff: {msg}\n{USAGE}");
    ExitCode::from(2)
}

fn verdict(errors: &[String], ok: &str) -> ExitCode {
    for e in errors {
        eprintln!("{e}");
    }
    if errors.is_empty() {
        println!("{ok}");
        ExitCode::SUCCESS
    } else {
        eprintln!("{} problem(s)", errors.len());
        ExitCode::from(1)
    }
}

/// Run `gcg-diff` with `args` (without the program name).
#[must_use]
pub fn main(args: Vec<String>) -> ExitCode {
    let Some((cmd, rest)) = args.split_first() else {
        return usage("no command");
    };
    let repo = Repo::locate();
    match cmd.as_str() {
        "corpus" => cmd_corpus(&repo, rest),
        "selftest" => cmd_selftest(&repo, rest),
        "lints" => cmd_lints(&repo, rest),
        "tests-json" => cmd_tests_json(&repo, rest),
        "weaknesses" => cmd_weaknesses(&repo, rest),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => match PENDING.iter().find(|(c, _)| *c == other) {
            Some((c, needs)) => usage(&format!("{c} is not in this build yet: it needs {needs}")),
            None => usage(&format!("unknown command {other:?}")),
        },
    }
}

fn cmd_corpus(repo: &Repo, rest: &[String]) -> ExitCode {
    let mut a = CorpusArgs {
        tier: 1,
        only: Vec::new(),
        file: None,
        engine: EngineKind::Mock,
        roundtrip: false,
        require_all: false,
        jobs: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        report: None,
        junit: None,
    };
    let mut engine: Option<EngineKind> = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().cloned();
        match flag.as_str() {
            "--tier" => match value().as_deref() {
                Some("1") => a.tier = 1,
                Some("2") => a.tier = 2,
                other => return usage(&format!("--tier takes 1 or 2, got {other:?}")),
            },
            "--only" => {
                let Some(v) = value() else {
                    return usage("--only needs SET[:FILTER]");
                };
                for part in v.split(',') {
                    let (set, filter) = match part.split_once(':') {
                        Some((s, f)) => (s.to_owned(), Some(f.to_owned())),
                        None => (part.to_owned(), None),
                    };
                    a.only.push((set, filter));
                }
            }
            "--file" => match value() {
                Some(v) => a.file = Some(PathBuf::from(v)),
                None => return usage("--file needs a path"),
            },
            "--parse-only" => engine = Some(EngineKind::None),
            "--mock" => engine = Some(EngineKind::Mock),
            "--roundtrip" => a.roundtrip = true,
            "--require-all" => a.require_all = true,
            "--jobs" => match value().and_then(|v| v.parse().ok()) {
                Some(n) if n > 0 => a.jobs = n,
                _ => return usage("--jobs needs a positive number"),
            },
            "--report" => match value() {
                Some(v) => a.report = Some(PathBuf::from(v)),
                None => return usage("--report needs a path"),
            },
            "--junit" => match value() {
                Some(v) => a.junit = Some(PathBuf::from(v)),
                None => return usage("--junit needs a path"),
            },
            "--paranoid" => {
                return usage(
                    "--paranoid replays on the engine's paranoid build (workstreams C, B2), not in this build yet",
                );
            }
            other => return usage(&format!("corpus: unknown option {other:?}")),
        }
    }
    let Some(engine) = engine else {
        return usage(
            "corpus replays against the real engine only once gcg-engine lands (workstreams C, B2); use --parse-only or --mock",
        );
    };
    a.engine = engine;
    if a.roundtrip && a.engine == EngineKind::None {
        return usage(
            "--roundtrip loads snapshots into an engine: combine it with --mock, not --parse-only",
        );
    }
    let report = corpus::run(repo, &a);
    if let Some(p) = &a.report {
        let text = serde_json::to_string_pretty(&corpus::report_json(&report)).unwrap_or_default();
        if let Err(e) = std::fs::write(p, text + "\n") {
            eprintln!("{}: {e}", p.display());
            return ExitCode::from(1);
        }
    }
    if let Some(p) = &a.junit
        && let Err(e) = std::fs::write(p, corpus::junit_xml(&report))
    {
        eprintln!("{}: {e}", p.display());
        return ExitCode::from(1);
    }
    let mode = match a.engine {
        EngineKind::None => "parse-only",
        EngineKind::Mock => "MockEngine",
    };
    println!(
        "corpus tier {} ({mode}{})",
        a.tier,
        if a.roundtrip { ", roundtrip" } else { "" }
    );
    println!("  {:<12} {:>7} {:>7}", "set", "traces", "passed");
    let mut total = (0, 0);
    for (set, n, ok) in corpus::per_set(&report) {
        println!("  {set:<12} {n:>7} {ok:>7}");
        total.0 += n;
        total.1 += ok;
    }
    println!("  {:<12} {:>7} {:>7}", "total", total.0, total.1);
    for r in report
        .results
        .iter()
        .filter(|r| r.outcome.is_err())
        .take(10)
    {
        if let Err(e) = &r.outcome {
            eprintln!("{e}");
        }
    }
    for e in &report.errors {
        eprintln!("{e}");
    }
    let failed = report.failed();
    if !report.errors.is_empty() || (a.require_all && (failed > 0 || total.0 == 0)) {
        eprintln!(
            "corpus check failed: {} integrity error(s), {failed} trace(s) diverged",
            report.errors.len()
        );
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn no_args(cmd: &str, rest: &[String]) -> Option<ExitCode> {
    (!rest.is_empty()).then(|| usage(&format!("{cmd} takes no options")))
}

fn cmd_selftest(repo: &Repo, rest: &[String]) -> ExitCode {
    if let Some(code) = no_args("selftest", rest) {
        return code;
    }
    match crate::selftest::run(&repo.root.join("port/corpus/v1/tier1")) {
        Ok(lines) => {
            for l in &lines {
                println!("{l}");
            }
            println!(
                "selftest passed: {} negative controls fired at their records",
                lines.len()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_lints(repo: &Repo, rest: &[String]) -> ExitCode {
    if let Some(code) = no_args("lints", rest) {
        return code;
    }
    match crate::lints::lint_repo(&repo.root) {
        Ok(findings) => {
            let errors: Vec<String> = findings.iter().map(ToString::to_string).collect();
            verdict(&errors, "lints passed")
        }
        Err(e) => verdict(&[e], ""),
    }
}

fn read(repo: &Repo, rel: &str) -> Result<String, String> {
    let p = repo.root.join(rel);
    std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))
}

fn cmd_tests_json(repo: &Repo, rest: &[String]) -> ExitCode {
    if rest != ["--check"] {
        return usage("tests-json takes --check");
    }
    match (
        read(repo, "docs/port/tests.json"),
        read(repo, "docs/port/PLAN.md"),
    ) {
        (Ok(t), Ok(p)) => verdict(&crate::testsjson::check(&t, &p), "tests.json check passed"),
        (Err(e), _) | (_, Err(e)) => verdict(&[e], ""),
    }
}

fn cmd_weaknesses(repo: &Repo, rest: &[String]) -> ExitCode {
    let mut file = None;
    let mut require = false;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--check" => file = it.next().cloned(),
            "--require-resolved" => require = true,
            other => return usage(&format!("weaknesses: unknown option {other:?}")),
        }
    }
    let Some(file) = file else {
        return usage("weaknesses takes --check FILE");
    };
    let weak = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) => return verdict(&[format!("{file}: {e}")], ""),
    };
    match read(repo, "docs/port/PLAN.md") {
        Ok(plan) => verdict(
            &crate::weaknesses::check(&weak, &plan, require),
            "weaknesses check passed",
        ),
        Err(e) => verdict(&[e], ""),
    }
}
