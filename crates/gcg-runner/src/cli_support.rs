//! The command bodies `gcg-sim` and `gcg-eval` share (`cli.py:102-370`; docs/port/contracts.md
//! open item O11): deck loading and its messages, `validate`, the progress printer,
//! the report-directory check, `benchmark` over any agent factory, `data status` and
//! `replay`. Each writes its stdout as bytes (paths keep their undecodable bytes, as Python's
//! UTF-8-mode stdout writes them with `surrogateescape`) and fails with a [`CliError`], which
//! `main` prints as `error: {message}` (exit 1) or as the interrupt message (exit 130,
//! `cli.py:394-399`).
//!
//! Paths arrive as the text argparse's `type=Path` produced (`str(Path(arg))`, a [`PyStr`]),
//! which is what Python prints in its messages.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gcg_core::pool::Cancel;
use gcg_core::pyfloat::format_fixed;
use gcg_core::pyjson::{DumpOptions, Json, dumps};
use gcg_data::{CardDb, Deck, DeckError, ImplementationStatus, ValidateOptions};
use gcg_effects::Registry;
use gcg_engine::MAX_ACTIONS_DEFAULT;
use serde_json::Value;

use crate::argparse::PyStr;
use crate::argparse::pypath::path_str;
use crate::config::DEFAULT_TURN_LIMIT;
use crate::replay::{check_replay, getitem, py_str_value, resimulate, seat_key};
use crate::reports::aggregate::Outcomes;
use crate::reports::write_reports;
use crate::run::run_benchmark;
use crate::{
    AgentFactory, BenchmarkConfig, Format, ReplayError, ReportError, RunError, SchemaVersion,
    Workers,
};

/// Why a command failed: `CliError(message)` (`cli.py:36-37`, printed as `error: {message}`,
/// exit 1) or `KeyboardInterrupt` (exit 130).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CliError {
    Invalid(PyStr),
    Interrupted,
}

impl CliError {
    fn text(s: &str) -> Self {
        Self::Invalid(PyStr::from(s))
    }
}

/// `str` concatenation of Python text and Rust text.
fn cat(parts: &[&PyStr]) -> PyStr {
    let mut out = PyStr::new();
    for p in parts {
        out.push_py(p);
    }
    out
}

fn t(s: &str) -> PyStr {
    PyStr::from(s)
}

/// `exc.strerror or exc`: the C library's text, without Rust's ` (os error N)` suffix.
fn strerror(error: &io::Error) -> String {
    let text = error.to_string();
    match error.raw_os_error() {
        Some(code) => text
            .strip_suffix(&format!(" (os error {code})"))
            .map_or_else(|| text.clone(), str::to_owned),
        None => text,
    }
}

/// `str(path / name)` for a normalised path text (only a root ends with `/`).
fn join(dir: &PyStr, name: &str) -> PyStr {
    let sep = if dir.code_points().last() == Some(&u32::from('/')) {
        ""
    } else {
        "/"
    };
    path_str(&cat(&[dir, &t(sep), &t(name)]))
}

/// `PurePosixPath(text).parent` of a normalised path text.
fn parent(path: &PyStr) -> PyStr {
    let cps = path.code_points();
    let slash = u32::from('/');
    let root = cps.iter().take_while(|&&c| c == slash).count().min(2);
    match cps.iter().rposition(|&c| c == slash) {
        Some(i) if i >= root => PyStr::from_code_points(cps[..i].to_vec()),
        _ if root > 0 => PyStr::from_code_points(cps[..root].to_vec()),
        _ => t("."),
    }
}

/// Writes `line` and a newline, ignoring write failures as `print` to a closed pipe would not;
/// the commands report only their own errors.
fn println(out: &mut dyn Write, line: &PyStr) {
    let mut bytes = line.to_surrogateescape_bytes();
    bytes.push(b'\n');
    let _ = out.write_all(&bytes);
}

// ---------------------------------------------------------------------------------------------
// decks (cli.py:102-136)

/// `_read_deck(path)` (`cli.py:102-110`): `load_deck`, with the three messages of
/// `OSError`, `DeckError` and `UnicodeDecodeError`.
///
/// # Errors
/// `cannot read deck file {path}: {strerror}`, `{path}: {DeckError}` or `{path}: not UTF-8
/// text ({reason} at byte {start})`.
pub fn read_deck(path: &PyStr) -> Result<Deck, CliError> {
    gcg_data::load_deck(Path::new(&path.to_os_string())).map_err(|e| {
        CliError::Invalid(match &e {
            DeckError::Io { error, .. } => cat(&[
                &t("cannot read deck file "),
                path,
                &t(": "),
                &t(&strerror(error)),
            ]),
            DeckError::Decode { reason, start, .. } => cat(&[
                path,
                &t(&format!(": not UTF-8 text ({reason} at byte {start})")),
            ]),
            DeckError::Invalid { .. } | DeckError::TooLarge { .. } => {
                cat(&[path, &t(": "), &t(&e.to_string())])
            }
        })
    })
}

/// `_deck_colors(deck)` (`cli.py:113-116`): the sorted colour values of the main deck, joined
/// with `/`, or `no colors`.
#[must_use]
pub fn deck_colors(deck: &Deck, db: &CardDb) -> String {
    let mut colors: Vec<&str> = deck
        .main
        .iter()
        .filter_map(|c| db.get(&c.card_number).and_then(|d| d.color))
        .map(|c| c.as_str())
        .collect();
    colors.sort();
    colors.dedup();
    if colors.is_empty() {
        "no colors".to_owned()
    } else {
        colors.join("/")
    }
}

/// `_legal_deck(path)` (`cli.py:119-126`): a readable deck with no violation of
/// `validate_deck(deck)` (the B&R list and the simulator-support check included).
///
/// # Errors
/// [`read_deck`]'s, or `{path}: {n} violation(s)` with one `  [{code}] {message}` line each.
pub fn legal_deck(path: &PyStr, implemented: &dyn ImplementationStatus) -> Result<Deck, CliError> {
    let deck = read_deck(path)?;
    let violations = gcg_data::validate(
        &deck,
        ValidateOptions {
            banlist: true,
            implemented: Some(implemented),
        },
    );
    if violations.is_empty() {
        return Ok(deck);
    }
    let mut text = cat(&[path, &t(&format!(": {} violation(s)", violations.len()))]);
    for v in &violations {
        text.push_str(&format!("\n  [{}] {}", v.code.as_str(), v.message));
    }
    Err(CliError::Invalid(text))
}

fn copies(cards: &[gcg_data::CardCount]) -> u128 {
    cards.iter().map(|c| c.count).sum()
}

/// `cmd_validate(args, out)` (`cli.py:129-136`): `{path}: legal ({n} cards, {m} resources;
/// {colors})`.
///
/// # Errors
/// [`legal_deck`]'s.
pub fn validate_command(path: &PyStr, out: &mut dyn Write) -> Result<(), CliError> {
    let reg = Registry::embedded();
    let deck = legal_deck(path, &*reg)?;
    let line = cat(&[
        path,
        &t(&format!(
            ": legal ({} cards, {} resources; {})",
            copies(&deck.main),
            copies(&deck.resources),
            deck_colors(&deck, reg.db())
        )),
    ]);
    println(out, &line);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// benchmark (cli.py:143-229)

/// `ProgressPrinter(stream)` (`cli.py:143-159`): match progress on stderr, in place on a
/// terminal, else about every 10 %.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ProgressPrinter {
    tty: bool,
    last_decile: Option<u64>,
}

impl ProgressPrinter {
    /// `tty` is `stream.isatty()`.
    #[must_use]
    pub const fn new(tty: bool) -> Self {
        Self {
            tty,
            last_decile: None,
        }
    }

    /// `__call__(done, total)`: on a terminal `\rmatches {done}/{total}`, with a newline after
    /// the last match; otherwise a `matches {done}/{total}` line whenever `10 * done // total`
    /// changes. Every write is flushed.
    pub fn print(&mut self, done: u32, total: u32, stream: &mut dyn Write) {
        if self.tty {
            let end = if done == total { "\n" } else { "" };
            let _ = write!(stream, "\rmatches {done}/{total}{end}");
            let _ = stream.flush();
            return;
        }
        let decile = 10 * u64::from(done) / u64::from(total.max(1));
        if self.last_decile != Some(decile) {
            self.last_decile = Some(decile);
            let _ = writeln!(stream, "matches {done}/{total}");
            let _ = stream.flush();
        }
    }
}

/// `_rate_line(label, block)` (`cli.py:167-173`) of a matches or games block whose `n` is at
/// least 1.
#[must_use]
pub fn rate_line(label: &str, block: &Outcomes) -> String {
    let r = &block.dut_win_rate;
    let rate = r.rate.unwrap_or(0.0);
    let (lo, hi) = r.ci95.unwrap_or((0.0, 0.0));
    format!(
        "{label}: DUT {}-{} (draws {}), win rate {}% [95% CI {}%-{}%] over {}",
        block.dut_wins,
        block.bench_wins,
        block.draws,
        format_fixed(100.0 * rate, 1),
        format_fixed(100.0 * lo, 1),
        format_fixed(100.0 * hi, 1),
        block.n
    )
}

/// `_writable_dir(path)` (`cli.py:176-185`): fail before simulating when the report directory
/// cannot be created or written. The walk goes up through `.parent` to the first path that
/// exists. Python asks `os.access(existing, W_OK | X_OK)`; safe Rust has no `access(2)`, so
/// this creates and at once removes an empty probe file in that directory instead, which is
/// possible exactly when the directory is writable and searchable. Nothing is left behind.
///
/// # Errors
/// `cannot write reports to {path}: {existing} is not a directory` or `... permission denied
/// on {existing}`.
pub fn writable_dir(path: &PyStr) -> Result<(), CliError> {
    let mut existing = path.clone();
    loop {
        if Path::new(&existing.to_os_string()).exists() {
            break;
        }
        let up = parent(&existing);
        if up == existing {
            break;
        }
        existing = up;
    }
    let dir = PathBuf::from(existing.to_os_string());
    let prefix = cat(&[&t("cannot write reports to "), path, &t(": "), &existing]);
    if !dir.is_dir() {
        return Err(CliError::Invalid(cat(&[
            &prefix,
            &t(" is not a directory"),
        ])));
    }
    let probe = dir.join(format!(".gcg-sim-write-probe.{}.tmp", std::process::id()));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(&probe);
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(_) => Err(CliError::Invalid(cat(&[
            &t("cannot write reports to "),
            path,
            &t(": permission denied on "),
            &existing,
        ]))),
    }
}

/// The parsed `benchmark` arguments (`cli.py:73-85`) plus what the binary fixes: the preset
/// name it writes, the results schema and the `versions()` inputs.
#[derive(Clone, Copy, Debug)]
pub struct BenchmarkRequest<'a> {
    pub deck_under_test: &'a PyStr,
    pub benchmark_deck: &'a PyStr,
    pub matches: u32,
    pub seed: u64,
    pub workers: Workers,
    pub format: Format,
    pub ai_preset: &'static str,
    pub out: &'a PyStr,
    pub decision_log: bool,
    pub replays: u32,
    pub schema: SchemaVersion,
    pub package: &'static str,
    pub code_digest: &'static str,
}

/// `cmd_benchmark(args, out, err)` (`cli.py:188-229`): both decks legal, the report directory
/// writable, the run (progress on `err`, `tty` = `err.isatty()`), the reports, then the
/// summary on `out`.
///
/// # Errors
/// The deck and directory messages; `simulation failed: {GameFailure}`; `cannot write reports
/// to {out}: {file}: {strerror}`; [`CliError::Interrupted`] on SIGINT.
pub fn benchmark_command(
    req: &BenchmarkRequest<'_>,
    factory: &dyn AgentFactory,
    tty: bool,
    cancel: &Cancel,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let reg = Registry::embedded();
    let dut = legal_deck(req.deck_under_test, &*reg)?;
    let bench = legal_deck(req.benchmark_deck, &*reg)?;
    writable_dir(req.out)?;
    let config = BenchmarkConfig {
        deck_under_test: dut,
        benchmark_deck: bench,
        matches: req.matches,
        seed: req.seed,
        workers: req.workers,
        format: req.format,
        ai_preset: req.ai_preset,
        out_dir: PathBuf::from(req.out.to_os_string()),
        decision_log: req.decision_log,
        replays: req.replays,
        turn_limit: DEFAULT_TURN_LIMIT,
        max_actions: MAX_ACTIONS_DEFAULT,
        schema: req.schema,
        package: req.package,
        code_digest: req.code_digest,
    };
    let mut printer = ProgressPrinter::new(tty);
    let run = run_benchmark(
        &config,
        factory,
        &mut |done, total| printer.print(done, total, err),
        cancel,
    )
    .map_err(|e| match e {
        RunError::Interrupted => CliError::Interrupted,
        RunError::GameFailure(m) => CliError::text(&format!("simulation failed: {m}")),
    })?;
    let paths = write_reports(&run, &config.out_dir, cancel, &mut |record| {
        resimulate(record, &run.config)
    })
    .map_err(|e| match e {
        ReportError::Interrupted => CliError::Interrupted,
        ReportError::Io { path, source } => CliError::Invalid(cat(&[
            &t("cannot write reports to "),
            req.out,
            &t(": "),
            &t(&gcg_data::py_path_str(&path)),
            &t(": "),
            &t(&strerror(&source)),
        ])),
        other => CliError::text(&other.to_string()),
    })?;
    println(
        out,
        &t(&format!(
            "{} (deck under test) vs {} (benchmark)",
            config.deck_under_test.name, config.benchmark_deck.name
        )),
    );
    println(
        out,
        &t(&rate_line("matches", &Outcomes::of_matches(&run.matches))),
    );
    println(
        out,
        &t(&rate_line("games", &Outcomes::of_games(run.games()))),
    );
    println(out, &t("reports:"));
    for (label, name) in [
        ("results", "results.json"),
        ("summary", "summary.md"),
        ("games", "games.ndjson"),
        ("timing", "timing.json"),
    ] {
        println(
            out,
            &cat(&[&t(&format!("  {label:<8} ")), &join(req.out, name)]),
        );
    }
    println(
        out,
        &cat(&[
            &t(&format!("  replays  {} file(s) in ", paths.replays.len())),
            &join(req.out, "replays"),
        ]),
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// data status (cli.py:236-331)

/// `_coverage()` (`cli.py:236-248`): real card numbers, those with a script, and both counts
/// per card type, types sorted.
#[must_use]
pub fn coverage(reg: &Registry) -> Json {
    let mut by_type: Vec<(&'static str, u64, u64)> = Vec::new();
    let (mut cards, mut implemented) = (0_u64, 0_u64);
    for c in reg.db().real_cards() {
        let done = reg.card(c.def_id).script.is_some();
        cards += 1;
        implemented += u64::from(done);
        let name = c.card_type.as_str();
        match by_type.iter_mut().find(|(t, _, _)| *t == name) {
            Some(entry) => {
                entry.1 += 1;
                entry.2 += u64::from(done);
            }
            None => by_type.push((name, 1, u64::from(done))),
        }
    }
    by_type.sort_by(|a, b| a.0.cmp(b.0));
    let mut types = Json::object();
    for (name, n, done) in by_type {
        let mut entry = Json::object();
        entry.set("cards", n);
        entry.set("implemented", done);
        types.set(name, entry);
    }
    let mut out = Json::object();
    out.set("card_numbers", cards);
    out.set("implemented", implemented);
    out.set("by_type", types);
    out
}

/// `data_status()` (`cli.py:263-291`).
#[must_use]
pub fn data_status(package: &str, code_digest: &str) -> Json {
    let parts = gcg_data::data_status_parts(package, code_digest);
    let mut out = Json::object();
    out.set("versions", parts.versions);
    out.set("snapshot", parts.snapshot);
    out.set("banlist", parts.banlist);
    out.set("coverage", coverage(&Registry::embedded()));
    out.set("conflicts", parts.conflicts);
    out
}

/// `str(v)` of a status value.
fn py_str(v: Option<&Json>) -> String {
    match v {
        None | Some(Json::Null) => "None".to_owned(),
        Some(Json::Str(s)) => s.clone(),
        Some(Json::Int(i)) => i.to_string(),
        Some(Json::Bool(b)) => if *b { "True" } else { "False" }.to_owned(),
        Some(other) => dumps(other, DumpOptions::PRETTY_SORTED_UTF8),
    }
}

fn len(v: Option<&Json>) -> usize {
    match v {
        Some(Json::Array(items)) => items.len(),
        Some(Json::Object(items)) => items.len(),
        _ => 0,
    }
}

fn int(v: Option<&Json>) -> i128 {
    match v {
        Some(Json::Int(i)) => *i,
        _ => 0,
    }
}

/// `_status_text(s)` (`cli.py:294-322`).
#[must_use]
pub fn status_text(s: &Json) -> Vec<String> {
    let part = |k: &str| s.get(k);
    let (v, snap, bl, cov, con) = (
        part("versions"),
        part("snapshot"),
        part("banlist"),
        part("coverage"),
        part("conflicts"),
    );
    let f = |block: Option<&Json>, key: &str| py_str(block.and_then(|b| b.get(key)));
    let card_numbers = int(cov.and_then(|c| c.get("card_numbers")));
    let implemented = int(cov.and_then(|c| c.get("implemented")));
    #[expect(
        clippy::cast_precision_loss,
        reason = "Python's int / int is the correctly rounded quotient; both counts are far \
                  below 2^53, where f64 division is that quotient"
    )]
    let pct = if card_numbers == 0 {
        0.0
    } else {
        (100 * implemented) as f64 / card_numbers as f64
    };
    let mut lines = vec![
        format!(
            "gcg-sim {} (results schema {})",
            f(v, "package"),
            f(v, "results_schema")
        ),
        format!("card data: gcg-api dataset {}", f(snap, "dataset_version")),
        format!(
            "  source commit {}, built {}",
            f(snap, "source_commit"),
            f(snap, "built_at")
        ),
        format!(
            "  {} printings, {card_numbers} card numbers, {} sets, {} rulings, {} rules FAQ \
             entries, {} errata",
            f(snap, "card_count"),
            f(snap, "set_count"),
            f(snap, "ruling_count"),
            f(snap, "rules_faq_count"),
            f(snap, "errata_count")
        ),
        format!(
            "rules: Comprehensive Rules Ver. {} (effective {})",
            f(v, "rules_version"),
            f(v, "rules_effective_date")
        ),
        format!(
            "banned & restricted list: effective {} ({} banned, {} restricted, {} banned pairs, \
             {} attribute pair rule(s))",
            f(bl, "effective_date"),
            len(bl.and_then(|b| b.get("banned"))),
            len(bl.and_then(|b| b.get("restricted"))),
            len(bl.and_then(|b| b.get("banned_pairs"))),
            len(bl.and_then(|b| b.get("attribute_pair_rules")))
        ),
        format!(
            "BO3 match rules: effective {}",
            f(v, "bo3_rules_effective_date")
        ),
        format!(
            "implementation coverage: {implemented}/{card_numbers} card numbers ({}%)",
            format_fixed(pct, 1)
        ),
    ];
    if let Some(Json::Object(types)) = cov.and_then(|c| c.get("by_type")) {
        for (name, c) in types {
            let name = match name {
                gcg_core::pyjson::Key::Str(s) => s.clone(),
                gcg_core::pyjson::Key::Int(i) => i.to_string(),
            };
            lines.push(format!(
                "  {name}: {}/{}",
                py_str(c.get("implemented")),
                py_str(c.get("cards"))
            ));
        }
    }
    lines.push(format!(
        "conflicts: {} resolutions in overrides.json ({} card-data overrides on {} card \
         numbers); {} curated conflicts",
        f(con, "resolutions"),
        f(con, "card_data_overrides"),
        f(con, "card_numbers_overridden"),
        f(con, "curated_conflicts")
    ));
    lines
}

/// `cmd_data_status(args, out)` (`cli.py:325-331`): the JSON (`indent=2, sort_keys=True,
/// ensure_ascii=False`) or the text lines.
pub fn data_status_command(json: bool, package: &str, code_digest: &str, out: &mut dyn Write) {
    let status = data_status(package, code_digest);
    let text = if json {
        dumps(&status, DumpOptions::PRETTY_SORTED_UTF8)
    } else {
        status_text(&status).join("\n")
    };
    println(out, &t(&text));
}

// ---------------------------------------------------------------------------------------------
// replay (cli.py:338-370)

const BOM: &str = "\u{feff}";

/// `path.read_text(encoding="utf-8")`: strict UTF-8 with CPython's error reason and offset,
/// then universal newlines.
fn read_text(path: &PyStr) -> Result<String, CliError> {
    let bytes = fs::read(PathBuf::from(path.to_os_string())).map_err(|e| {
        CliError::Invalid(cat(&[
            &t("cannot read replay file "),
            path,
            &t(": "),
            &t(&strerror(&e)),
        ]))
    })?;
    let text = std::str::from_utf8(&bytes).map_err(|e| {
        let start = e.valid_up_to();
        let reason = match e.error_len() {
            None => "unexpected end of data",
            Some(_) if matches!(bytes[start], 0x80..=0xC1 | 0xF5..=0xFF) => "invalid start byte",
            Some(_) => "invalid continuation byte",
        };
        CliError::Invalid(cat(&[
            path,
            &t(&format!(": not UTF-8 text ({reason} at byte {start})")),
        ]))
    })?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `json.loads(text)`. A leading BOM is Python's `JSONDecodeError` exactly; any other syntax
/// error keeps Python's prefix and gives serde_json's description (PLAN A19).
fn loads(path: &PyStr, text: &str) -> Result<Value, CliError> {
    let invalid =
        |detail: &str| CliError::Invalid(cat(&[path, &t(" is not valid JSON: "), &t(detail)]));
    if text.starts_with(BOM) {
        return Err(invalid(
            "Unexpected UTF-8 BOM (decode using utf-8-sig): line 1 column 1 (char 0)",
        ));
    }
    serde_json::from_str(text).map_err(|e| invalid(&e.to_string()))
}

/// `cmd_replay(args, out)` (`cli.py:338-370`): read and check the replay, print the log
/// (with `--log`), the seats and the result, then whether it matches.
///
/// # Errors
/// The read, decode and JSON messages; `{path} is not a replay document`; `cannot replay
/// {path}: {exc}`; `replay does NOT match the recorded outcome:` with one line per mismatch.
pub fn replay_command(path: &PyStr, log: bool, out: &mut dyn Write) -> Result<(), CliError> {
    let text = read_text(path)?;
    let doc = loads(path, &text)?;
    if !doc.is_object() {
        return Err(CliError::Invalid(cat(&[
            path,
            &t(" is not a replay document"),
        ])));
    }
    let cannot = |e: &ReplayError| {
        CliError::Invalid(cat(&[
            &t("cannot replay "),
            path,
            &t(": "),
            &t(&e.to_string()),
        ]))
    };
    let check = check_replay(&doc).map_err(|e| cannot(&e))?;
    if log {
        println(out, &t(&check.log.join("\n")));
    }
    // check_replay has sorted the same seats already, so the sort keys exist. Python looks
    // the printed fields up outside any `try` (a missing one is a traceback, exit 1).
    let mut seats: Vec<(i128, &Value)> = Vec::new();
    for seat in doc["seats"].as_array().map_or(&[][..], Vec::as_slice) {
        seats.push((seat_key(seat).map_err(|e| cannot(&e))?, seat));
    }
    seats.sort_by_key(|(key, _)| *key);
    for (_, seat) in seats {
        let mut fields = Vec::with_capacity(4);
        for key in ["seat", "role", "deck", "agent"] {
            fields.push(py_str_value(getitem(seat, key).map_err(|e| cannot(&e))?));
        }
        println(
            out,
            &t(&format!(
                "seat {}: {} {} ({})",
                fields[0], fields[1], fields[2], fields[3]
            )),
        );
    }
    let winner = check
        .winner_seat
        .map_or_else(|| "None".to_owned(), |w| w.to_string());
    let reason = check.end_reason.map_or("None", |r| r.as_str());
    println(
        out,
        &t(&format!(
            "result: winner seat {winner} ({reason}) after {} turns",
            check.turns
        )),
    );
    if !check.ok {
        return Err(CliError::text(&format!(
            "replay does NOT match the recorded outcome:\n  {}",
            check.mismatches.join("\n  ")
        )));
    }
    println(
        out,
        &t("replay matches the recorded outcome and final state"),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printed(tty: bool, total: u32) -> String {
        let mut p = ProgressPrinter::new(tty);
        let mut buf = Vec::new();
        for done in 1..=total {
            p.print(done, total, &mut buf);
        }
        String::from_utf8(buf).expect("ASCII")
    }

    #[test]
    fn progress_prints_like_python() {
        // Inv 01 §3.6: totals 3, 25 and 100 off a terminal.
        assert_eq!(printed(false, 3), "matches 1/3\nmatches 2/3\nmatches 3/3\n");
        let lines: Vec<String> = printed(false, 25)
            .lines()
            .map(|l| l.trim_start_matches("matches ").to_owned())
            .collect();
        assert_eq!(
            lines,
            [
                "1/25", "3/25", "5/25", "8/25", "10/25", "13/25", "15/25", "18/25", "20/25",
                "23/25", "25/25"
            ]
        );
        let hundred = printed(false, 100);
        assert_eq!(hundred.lines().count(), 11);
        assert!(hundred.starts_with("matches 1/100\nmatches 10/100\nmatches 20/100\n"));
        assert_eq!(printed(true, 2), "\rmatches 1/2\rmatches 2/2\n");
    }

    #[test]
    fn rate_lines_round_like_python() {
        let block = Outcomes::of([
            crate::Outcome::Dut,
            crate::Outcome::Bench,
            crate::Outcome::Bench,
        ]);
        assert_eq!(
            rate_line("matches", &block),
            "matches: DUT 1-2 (draws 0), win rate 33.3% [95% CI 6.1%-79.2%] over 3"
        );
    }

    #[test]
    fn parents_follow_purepath() {
        let p = |s: &str| parent(&t(s)).to_string_opt().expect("UTF-8");
        assert_eq!(p("a/b/c"), "a/b");
        assert_eq!(p("a"), ".");
        assert_eq!(p("."), ".");
        assert_eq!(p("/"), "/");
        assert_eq!(p("/a"), "/");
        assert_eq!(p("//x"), "//");
        assert_eq!(p("//"), "//");
        assert_eq!(p("/a/b"), "/a");
        let j = |d: &str, n: &str| join(&t(d), n).to_string_opt().expect("UTF-8");
        assert_eq!(j(".", "results.json"), "results.json");
        assert_eq!(j("out", "replays"), "out/replays");
        assert_eq!(j("/", "x"), "/x");
    }

    #[test]
    fn strerror_drops_the_os_error_suffix() {
        let e = io::Error::from_raw_os_error(2);
        assert_eq!(strerror(&e), "No such file or directory");
        assert_eq!(strerror(&io::Error::other("custom")), "custom");
    }
}
