//! The `gcg-diff` command line (PLAN §8.7, §9.2-§9.5, §9.9-§9.10, §12.2). Exit codes: 0
//! success, 1 a check failed or a replay diverged, 2 usage.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::corpus::{self, CorpusArgs, EngineKind};
use crate::manifest::Repo;

const USAGE: &str = "\
usage: gcg-diff <command> [options]

commands:
  corpus [--tier 1|2] [--only SET[:FILTER] ...] [--file TRACE.ndjson] [--parse-only | --mock]
         [--roundtrip] [--paranoid] [--require-all] [--jobs N] [--report FILE] [--junit FILE]
  explain <trace-id> <decision> [--tier 1|2] [--file TRACE.ndjson] [--limit N]
  trace-search <trace-id> <decision> [--level 2|3] [--iteration N] [--tier 1|2] [--file TRACE.ndjson]
  selftest [--real-engine]
  fuzz --games N (--seed S | --seed-from-commit SHA) [--workers W|auto] [--compare FILE]
       [--agent random|greedy|ref:<preset>|search:<preset>|mixed:<spec>,<spec>] [--paranoid] [--show N]
  paranoid-games
  coverage --check
  traceability (--check | --write) [--junit FILE]
  tests-json --check
  schema-check [--expect-invalid] FILE...
  weaknesses --check FILE [--require-resolved]
  lints
  api-snapshot (--check | --write)
";

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
        "explain" => cmd_explain(&repo, rest),
        "trace-search" => cmd_trace_search(&repo, rest),
        "selftest" => cmd_selftest(&repo, rest),
        "fuzz" => cmd_fuzz(rest),
        "paranoid-games" => cmd_paranoid_games(rest),
        "coverage" => cmd_coverage(&repo, rest),
        "traceability" => cmd_traceability(&repo, rest),
        "schema-check" => cmd_schema_check(rest),
        "lints" => cmd_lints(&repo, rest),
        "tests-json" => cmd_tests_json(&repo, rest),
        "weaknesses" => cmd_weaknesses(&repo, rest),
        "api-snapshot" => cmd_api_snapshot(&repo, rest),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => usage(&format!("unknown command {other:?}")),
    }
}

/// The positional arguments and `--flag value` options of a command.
struct Opts {
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

/// Splits `rest` into positionals and options; `valued` lists the options that take a value.
fn opts(rest: &[String], valued: &[&str], bare: &[&str]) -> Result<Opts, String> {
    let mut o = Opts {
        positional: Vec::new(),
        flags: Vec::new(),
    };
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if valued.contains(&a.as_str()) {
            let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
            o.flags.push((a.clone(), Some(v.clone())));
        } else if bare.contains(&a.as_str()) {
            o.flags.push((a.clone(), None));
        } else if a.starts_with("--") {
            return Err(format!("unknown option {a:?}"));
        } else {
            o.positional.push(a.clone());
        }
    }
    Ok(o)
}

impl Opts {
    fn value(&self, flag: &str) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(f, _)| f == flag)
            .and_then(|(_, v)| v.as_deref())
    }

    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|(f, _)| f == flag)
    }

    fn tier(&self) -> Result<u8, String> {
        match self.value("--tier") {
            None | Some("1") => Ok(1),
            Some("2") => Ok(2),
            Some(other) => Err(format!("--tier takes 1 or 2, got {other:?}")),
        }
    }

    fn number<T: std::str::FromStr>(&self, flag: &str, default: T) -> Result<T, String> {
        self.value(flag).map_or(Ok(default), |v| {
            v.parse()
                .map_err(|_| format!("{flag} takes a number, got {v:?}"))
        })
    }
}

fn print_lines(lines: &[String]) {
    for l in lines {
        println!("{l}");
    }
}

fn cmd_explain(repo: &Repo, rest: &[String]) -> ExitCode {
    let o = match opts(rest, &["--tier", "--file", "--limit"], &[]) {
        Ok(o) => o,
        Err(e) => return usage(&format!("explain: {e}")),
    };
    let ([id, k], Ok(tier), Ok(limit)) =
        (o.positional.as_slice(), o.tier(), o.number("--limit", 60))
    else {
        return usage("explain takes <trace-id> <decision>");
    };
    let Ok(k) = k.parse::<u32>() else {
        return usage("explain: the decision is a number");
    };
    match crate::explain::explain(repo, tier, o.value("--file").map(Path::new), id, k, limit) {
        Ok(lines) => {
            print_lines(&lines);
            ExitCode::SUCCESS
        }
        Err(e) => verdict(&[e], ""),
    }
}

fn cmd_trace_search(repo: &Repo, rest: &[String]) -> ExitCode {
    let o = match opts(rest, &["--tier", "--file", "--level", "--iteration"], &[]) {
        Ok(o) => o,
        Err(e) => return usage(&format!("trace-search: {e}")),
    };
    let [id, k] = o.positional.as_slice() else {
        return usage("trace-search takes <trace-id> <decision>");
    };
    let (Ok(decision), Ok(tier)) = (k.parse::<u32>(), o.tier()) else {
        return usage("trace-search: the decision is a number");
    };
    let level = match o.value("--level") {
        None | Some("2") => 2,
        Some("3") => 3,
        Some(other) => return usage(&format!("--level takes 2 or 3, got {other:?}")),
    };
    let iteration = match o.value("--iteration").map(str::parse::<u32>).transpose() {
        Ok(n) => n,
        Err(_) => return usage("--iteration takes a number"),
    };
    let req = crate::tracesearch::Request {
        tier,
        file: o.value("--file").map(Path::new),
        id,
        decision,
        level,
        iteration,
    };
    match crate::tracesearch::trace_search(repo, &req) {
        Ok((lines, same)) => {
            print_lines(&lines);
            if same {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => verdict(&[e], ""),
    }
}

fn paranoid_build() -> Option<ExitCode> {
    (!cfg!(feature = "paranoid")).then(|| {
        usage(
            "--paranoid needs the paranoid build: cargo build --release -p gcg-diff \
             --features paranoid --target-dir target/paranoid",
        )
    })
}

fn cmd_fuzz(rest: &[String]) -> ExitCode {
    use crate::fuzz::{self, AgentKind, AgentSpec, FuzzArgs};
    let o = match opts(
        rest,
        &[
            "--games",
            "--seed",
            "--seed-from-commit",
            "--workers",
            "--compare",
            "--agent",
            "--show",
        ],
        &["--paranoid"],
    ) {
        Ok(o) if o.positional.is_empty() => o,
        Ok(_) => return usage("fuzz takes no positional arguments"),
        Err(e) => return usage(&format!("fuzz: {e}")),
    };
    if o.has("--paranoid")
        && let Some(code) = paranoid_build()
    {
        return code;
    }
    let games: u64 = match o.number("--games", 1000) {
        Ok(n) => n,
        Err(e) => return usage(&e),
    };
    let seed = match (o.value("--seed"), o.value("--seed-from-commit")) {
        (Some(s), None) => match s.parse::<u64>() {
            Ok(s) => s,
            Err(_) => return usage(&format!("--seed takes a number, got {s:?}")),
        },
        (None, Some(sha)) => {
            let s = fuzz::commit_seed(sha);
            println!("seed {s} from commit {sha}");
            s
        }
        (None, None) => 1,
        (Some(_), Some(_)) => return usage("give --seed or --seed-from-commit, not both"),
    };
    let workers = match o.value("--workers") {
        None => NonZeroUsize::MIN,
        Some("auto") => {
            let n = gcg_core::sys::topology::detect()
                .auto_workers(usize::try_from(games).unwrap_or(usize::MAX));
            NonZeroUsize::new(n).unwrap_or(NonZeroUsize::MIN)
        }
        Some(w) => match w.parse::<usize>().ok().and_then(NonZeroUsize::new) {
            Some(n) => n,
            None => {
                return usage(&format!(
                    "--workers takes a positive number or auto, got {w:?}"
                ));
            }
        },
    };
    let agent = match o.value("--agent").map(fuzz::parse_agent) {
        None => AgentSpec::Both(AgentKind::Fuzz),
        Some(Ok(a)) => a,
        Some(Err(e)) => return usage(&format!("--agent: {e}")),
    };
    let show: usize = match o.number("--show", 5) {
        Ok(n) => n,
        Err(e) => return usage(&e),
    };
    let args = FuzzArgs {
        games,
        seed,
        workers,
        agent,
    };
    let outs = match fuzz::run(&args) {
        Ok(o) => o,
        Err(e) => return verdict(&[e], ""),
    };
    let reg = gcg_effects::Registry::embedded();
    let text = fuzz::summary_text(&outs, &fuzz::Pools::new(&reg));
    print!("{text}");
    for l in fuzz::failure_lines(&outs, show) {
        eprintln!("{l}");
    }
    let mut errors = Vec::new();
    if let Some(f) = o.value("--compare")
        && let Err(e) = fuzz::compare(&text, &outs, Path::new(f))
    {
        errors.push(e);
    }
    let failed = outs.iter().filter(|g| !g.ok).count();
    if failed > 0 {
        errors.push(format!("{failed} game(s) failed"));
    }
    verdict(&errors, "fuzz passed")
}

fn cmd_paranoid_games(rest: &[String]) -> ExitCode {
    if let Some(code) = no_args("paranoid-games", rest) {
        return code;
    }
    if let Some(code) = paranoid_build() {
        return code;
    }
    match crate::paranoid::run() {
        Ok((lines, ok)) => {
            print_lines(&lines);
            if ok {
                println!("paranoid-games passed");
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => verdict(&[e], ""),
    }
}

fn cmd_coverage(repo: &Repo, rest: &[String]) -> ExitCode {
    if rest != ["--check"] {
        return usage("coverage takes --check");
    }
    match crate::coverage::check(repo) {
        Ok((lines, problems)) => {
            print_lines(&lines);
            let shown: Vec<String> = problems.iter().take(40).cloned().collect();
            if problems.len() > shown.len() {
                eprintln!("(first {} of {} problems)", shown.len(), problems.len());
            }
            if problems.is_empty() {
                verdict(&[], "coverage check passed")
            } else {
                verdict(&shown, "")
            }
        }
        Err(e) => verdict(&[e], ""),
    }
}

fn cmd_traceability(repo: &Repo, rest: &[String]) -> ExitCode {
    let o = match opts(rest, &["--junit"], &["--check", "--write"]) {
        Ok(o) if o.positional.is_empty() => o,
        Ok(_) => return usage("traceability takes no positional arguments"),
        Err(e) => return usage(&format!("traceability: {e}")),
    };
    let write = match (o.has("--check"), o.has("--write")) {
        (true, false) => false,
        (false, true) => true,
        _ => return usage("traceability takes --check or --write"),
    };
    match crate::traceability::run(repo, o.value("--junit").map(Path::new), write) {
        Ok((lines, problems)) => {
            print_lines(&lines);
            let shown: Vec<String> = problems.iter().take(40).cloned().collect();
            if problems.len() > shown.len() {
                eprintln!("(first {} of {} problems)", shown.len(), problems.len());
            }
            if problems.is_empty() {
                verdict(&[], "traceability passed")
            } else {
                verdict(&shown, "")
            }
        }
        Err(e) => verdict(&[e], ""),
    }
}

fn cmd_schema_check(rest: &[String]) -> ExitCode {
    let expect_invalid = rest.first().is_some_and(|a| a == "--expect-invalid");
    let files: Vec<String> = rest
        .iter()
        .skip(usize::from(expect_invalid))
        .cloned()
        .collect();
    if let Some(bad) = files.iter().find(|f| f.starts_with("--")) {
        return usage(&format!("schema-check: unknown option {bad:?}"));
    }
    let (lines, errors) = crate::schema::check(&files, expect_invalid);
    print_lines(&lines);
    verdict(&errors, "schema-check passed")
}

fn cmd_corpus(repo: &Repo, rest: &[String]) -> ExitCode {
    let mut a = CorpusArgs {
        tier: 1,
        only: Vec::new(),
        file: None,
        engine: EngineKind::Real,
        roundtrip: false,
        paranoid: false,
        require_all: false,
        jobs: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        report: None,
        junit: None,
        census: false,
    };
    let mut engine = EngineKind::Real;
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
            "--parse-only" => engine = EngineKind::None,
            "--mock" => engine = EngineKind::Mock,
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
                if !cfg!(feature = "paranoid") {
                    return usage(
                        "--paranoid needs the paranoid build: cargo build --release -p gcg-diff \
                         --features paranoid --target-dir target/paranoid",
                    );
                }
                a.paranoid = true;
            }
            other => return usage(&format!("corpus: unknown option {other:?}")),
        }
    }
    a.engine = engine;
    if a.roundtrip && a.engine == EngineKind::None {
        return usage(
            "--roundtrip loads snapshots into an engine: it cannot be combined with --parse-only",
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
        EngineKind::Real => "gcg-engine",
    };
    println!(
        "corpus tier {} ({mode}{}{})",
        a.tier,
        if a.roundtrip { ", roundtrip" } else { "" },
        if a.paranoid { ", paranoid" } else { "" }
    );
    println!(
        "  {:<12} {:>7} {:>7} {:>9} {:>9} {:>7}",
        "set", "traces", "passed", "decisions", "snapshots", "agent"
    );
    let mut total = (0, 0, 0, 0, 0);
    for ((set, n, ok), (_, st)) in corpus::per_set(&report)
        .into_iter()
        .zip(corpus::set_stats(&report))
    {
        println!(
            "  {set:<12} {n:>7} {ok:>7} {:>9} {:>9} {:>7}",
            st.decisions, st.snapshots, st.agent_decisions
        );
        total.0 += n;
        total.1 += ok;
        total.2 += st.decisions;
        total.3 += st.snapshots;
        total.4 += st.agent_decisions;
    }
    println!(
        "  {:<12} {:>7} {:>7} {:>9} {:>9} {:>7}",
        "total", total.0, total.1, total.2, total.3, total.4
    );
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
    let real = match rest {
        [] => false,
        [flag] if flag == "--real-engine" => true,
        _ => return usage("selftest takes no options but --real-engine"),
    };
    let tier1 = repo.root.join("oracle/corpus/v1/tier1");
    let result = if real {
        crate::selftest::run_real(&tier1)
    } else {
        crate::selftest::run(&tier1)
    };
    match result {
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

fn cmd_api_snapshot(repo: &Repo, rest: &[String]) -> ExitCode {
    use crate::apisnapshot::{SNAPSHOT_FILE, check, write};
    match rest {
        [flag] if flag == "--check" => match check(&repo.root) {
            Ok(problems) => verdict(&problems, "api-snapshot check passed"),
            Err(e) => verdict(&[e], ""),
        },
        [flag] if flag == "--write" => match write(&repo.root) {
            Ok(lines) => verdict(&[], &format!("wrote {SNAPSHOT_FILE} ({lines} lines)")),
            Err(e) => verdict(&[e], ""),
        },
        _ => usage("api-snapshot takes --check or --write"),
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
        read(repo, "docs/dev/tests.json"),
        read(repo, "docs/dev/PLAN.md"),
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
    match read(repo, "docs/dev/PLAN.md") {
        Ok(plan) => verdict(
            &crate::weaknesses::check(&weak, &plan, require),
            "weaknesses check passed",
        ),
        Err(e) => verdict(&[e], ""),
    }
}
