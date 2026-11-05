//! `gcg-eval validate DECK [--as-of DATE] [--region R] [--no-banlist] [--no-implemented]
//! [--event-date DATE] [--scope]` (docs/dev/PLAN.md A30, §12.2 P6-L): the opt-in deck report
//! of W-71 to W-78 over [`gcg_data::legality`]. `gcg-sim validate` stays Python's (A30).
//!
//! The report runs `validate_deck`'s checks with Python's messages and order, and adds:
//!
//! - the list applied: the current list by default, the list in effect on `--as-of` in
//!   `--region` (W-72), or none with `--no-banlist`, which puts a warning in the header (W-75);
//! - the rules violations apart from the simulator-support ones, which `--no-implemented`
//!   skips for validation-only use (W-78; the switch of `validate_deck(implemented=False)`,
//!   added to A30's list because W-78's plan needs it on the command line);
//! - the 7-day new-card check for an event on `--event-date` (W-76);
//! - the starter-deck exceptions applied through the data hook (W-71), and hints (W-77);
//! - with `--scope`, what the validator does not check (W-76).
//!
//! The first line of a legal deck is `gcg-sim validate`'s line. A deck that fails a check
//! prints its report on stdout, then `error: {path}: {n} violation(s)` on stderr, exit 1. A
//! deck that does not parse prints its hints (if any) on stdout and `gcg-sim validate`'s error
//! on stderr, exit 1.
//!
//! Two packaged files that the Python runtime does not read are embedded here, not in
//! gcg-data, whose embedded set is exactly the Python runtime's (docs/dev/PLAN.md §2.4):
//! `gcgapi/products.json` (release dates, `--event-date`) and `official/edition_language.json`
//! (the Edition Beta facts of `--scope`). Both are read-only sources.

use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

use gcg_data::legality::{
    FORMAT_APPLIED, IsoDate, LegalityOptions, LegalityReport, LegalitySources, ListRequest,
    ReleaseDates, Scope, UNSUPPORTED_FORMATS, check, file_hints,
};
use gcg_data::{CardCount, Deck, ImplementationStatus, Violation};
use gcg_effects::Registry;
use gcg_runner::argparse::{Action, Converter, Exit, Namespace, Parser, PyStr, Value};
use gcg_runner::cli_support::{CliError, deck_colors, read_deck};

use crate::bench_cmd::CommandError;

const PRODUCTS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/gcg_sim/data/gcgapi/products.json"
));
const EDITION_LANGUAGE_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/gcg_sim/data/official/edition_language.json"
));

/// The release dates of the packaged `gcgapi/products.json`.
///
/// # Panics
/// If the embedded file does not load (a build defect; this crate's tests load it).
#[must_use]
pub fn packaged_releases() -> &'static ReleaseDates {
    static DATES: OnceLock<ReleaseDates> = OnceLock::new();
    DATES.get_or_init(|| {
        ReleaseDates::from_json(PRODUCTS_JSON)
            .unwrap_or_else(|e| panic!("embedded products.json does not load: {e}"))
    })
}

/// The validator's scope over the packaged data; [`Scope::to_json`] is the `scope` block of
/// `analysis.json` (W-75, W-76).
///
/// # Panics
/// If an embedded file lacks a field the scope reads (a build defect; this crate's tests
/// build it).
#[must_use]
pub fn packaged_scope() -> &'static Scope {
    static SCOPE: OnceLock<Scope> = OnceLock::new();
    SCOPE.get_or_init(|| {
        Scope::new(EDITION_LANGUAGE_JSON)
            .unwrap_or_else(|e| panic!("embedded scope sources do not load: {e}"))
    })
}

/// The parsed `validate` arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ValidateArgs {
    /// `str(Path(text))`, as `gcg-sim validate` prints it.
    pub deck: PyStr,
    pub as_of: Option<IsoDate>,
    pub region: Option<String>,
    pub no_banlist: bool,
    pub no_implemented: bool,
    pub event_date: Option<IsoDate>,
    pub scope: bool,
}

const DESCRIPTION: &str = "The deck report of W-71 to W-78: gcg-sim validate's checks with the \
     Banned & Restricted list chosen by date and region, the rules apart from simulator support, \
     the optional 7-day new-card rule, hints and the validator's scope.";

/// The `validate` subcommand parser; `--region` offers the regions of the packaged B&R data.
#[must_use]
pub fn parser(prog: &str) -> Parser {
    let mut p = Parser::new(&format!("{prog} validate"), Some(DESCRIPTION));
    p.push(Action::positional(
        "deck",
        Converter::Path,
        Some("deck file".to_owned()),
    ));
    let date_option = |option: &'static str, dest: &'static str, help: &str| {
        Action::option(
            option,
            dest,
            Converter::Str,
            None,
            Value::None,
            help.to_owned(),
        )
    };
    p.push(date_option(
        "--as-of",
        "as_of",
        "apply the Banned & Restricted list in effect on this date (YYYY-MM-DD)",
    ));
    let regions = gcg_data::legality::BanlistHistory::packaged().regions();
    p.push(Action::option(
        "--region",
        "region",
        Converter::Str,
        Some(regions),
        Value::None,
        "apply the list of this region of the official site (en: North America, Latin America, \
         Europe, Oceania; asia-en: English Asia)"
            .to_owned(),
    ));
    p.push(Action::flag(
        "--no-banlist",
        "no_banlist",
        "check the Comprehensive Rules only: no Banned & Restricted list".to_owned(),
    ));
    p.push(Action::flag(
        "--no-implemented",
        "no_implemented",
        "skip the simulator-support check (validation only: the deck may not be playable)"
            .to_owned(),
    ));
    p.push(date_option(
        "--event-date",
        "event_date",
        "also check the 7-day new-card rule for an event on this date (YYYY-MM-DD)",
    ));
    p.push(Action::flag(
        "--scope",
        "scope",
        "also list what the validator does not check".to_owned(),
    ));
    p
}

fn text(ns: &Namespace, dest: &str) -> Option<String> {
    match ns.get(dest) {
        Some(Value::Str(s)) => Some(
            s.to_string_opt()
                .unwrap_or_else(|| String::from_utf8_lossy(&s.to_surrogateescape_bytes()).into()),
        ),
        _ => None,
    }
}

fn flag(ns: &Namespace, dest: &str) -> bool {
    matches!(ns.get(dest), Some(Value::Bool(true)))
}

/// The typed arguments of a parsed `validate` namespace.
///
/// # Errors
/// A usage error (exit 2) for a date that is not `YYYY-MM-DD`, or `--no-banlist` with
/// `--as-of` or `--region`.
pub fn args(ns: &Namespace, sub: &Parser, columns: i64) -> Result<ValidateArgs, Exit> {
    let usage = |msg: String| sub.error(&PyStr::from(msg.as_str()), columns);
    let date = |dest: &str, option: &str| -> Result<Option<IsoDate>, Exit> {
        match text(ns, dest) {
            None => Ok(None),
            Some(t) => IsoDate::parse(&t).map(Some).ok_or_else(|| {
                usage(format!(
                    "argument {option}: invalid date value: {t:?} (expected YYYY-MM-DD)"
                ))
            }),
        }
    };
    let deck = match ns.get("deck") {
        Some(Value::Path(p)) => p.clone(),
        _ => unreachable!("deck is a required Path argument"),
    };
    let out = ValidateArgs {
        deck,
        as_of: date("as_of", "--as-of")?,
        region: text(ns, "region"),
        no_banlist: flag(ns, "no_banlist"),
        no_implemented: flag(ns, "no_implemented"),
        event_date: date("event_date", "--event-date")?,
        scope: flag(ns, "scope"),
    };
    if out.no_banlist {
        let other = if out.as_of.is_some() {
            Some("--as-of")
        } else if out.region.is_some() {
            Some("--region")
        } else {
            None
        };
        if let Some(o) = other {
            return Err(usage(format!(
                "argument --no-banlist: not allowed with argument {o}"
            )));
        }
    }
    Ok(out)
}

fn cli_text(e: &CliError) -> String {
    match e {
        CliError::Invalid(s) => s
            .to_string_opt()
            .unwrap_or_else(|| String::from_utf8_lossy(&s.to_surrogateescape_bytes()).into()),
        CliError::Interrupted => gcg_runner::INTERRUPTED_MESSAGE.to_owned(),
    }
}

fn copies(cards: &[CardCount]) -> u128 {
    cards.iter().map(|c| c.count).sum()
}

fn section(lines: &mut Vec<String>, title: &str, violations: &[Violation]) {
    if violations.is_empty() {
        lines.push(format!("{title}: no violations"));
        return;
    }
    lines.push(format!("{title}: {} violation(s)", violations.len()));
    lines.extend(
        violations
            .iter()
            .map(|v| format!("  [{}] {}", v.code.as_str(), v.message)),
    );
}

fn list_line(args: &ValidateArgs, report: &LegalityReport) -> String {
    let Some(e) = &report.banlist else {
        return "banned & restricted list: none (--no-banlist)".to_owned();
    };
    let mut how = Vec::new();
    if e.current {
        how.push("the current list".to_owned());
    }
    if let Some(d) = args.as_of {
        how.push(format!("in effect on {d}"));
    }
    if let Some(r) = &args.region {
        how.push(format!("region {r}"));
    }
    format!(
        "banned & restricted list: effective {} ({}; regions {})",
        e.list.effective_date,
        how.join(", "),
        e.regions.join(", ")
    )
}

fn scope_lines(lines: &mut Vec<String>, args: &ValidateArgs, deck: &Deck) {
    let scope = packaged_scope();
    lines.push("scope:".to_owned());
    lines.push(format!(
        "  format applied: {FORMAT_APPLIED} ({}; the Banned & Restricted list applies to the {})",
        scope.banlist_format, scope.banlist_applies_to
    ));
    lines.push("  formats not supported:".to_owned());
    for (name, description) in UNSUPPORTED_FORMATS {
        lines.push(format!("    - {name}: {description}"));
    }
    lines.push("  tournament deck rules not checked:".to_owned());
    for r in scope.unchecked_rules.iter() {
        lines.push(format!("    - {r}"));
    }
    let seven = match args.event_date {
        Some(d) => format!("checked in this run for an event on {d}"),
        None => "not checked in this run".to_owned(),
    };
    lines.push(format!(
        "    (the 7-day new-card rule is checked only with --event-date: {seven})"
    ));
    lines.push("  tournament procedures not modelled:".to_owned());
    for (k, v) in scope.unmodelled_procedures.iter() {
        lines.push(format!("    - {k}: {v}"));
    }
    let statement = if scope.edition_beta_statement_found {
        "an official statement exists"
    } else {
        "no official statement on their tournament legality was found"
    };
    lines.push(format!(
        "  Edition Beta: {statement}; printings are validated by card number. Card numbers \
         printed only in Edition Beta: {}",
        scope.edition_beta_only.join(", ")
    ));
    let used = scope.edition_beta_only_in(deck);
    if used.is_empty() {
        lines.push("  this deck uses no Edition Beta-only card number".to_owned());
    } else {
        lines.push(format!(
            "  FLAG: this deck uses Edition Beta-only card numbers, whose tournament legality is \
             undecided: {}",
            used.join(", ")
        ));
    }
}

/// The report lines of a parsed deck (the first is `gcg-sim validate`'s line when it is legal).
#[must_use]
pub fn render(args: &ValidateArgs, deck: &Deck, report: &LegalityReport) -> Vec<String> {
    let path = cli_text(&CliError::Invalid(args.deck.clone()));
    let mut lines = Vec::new();
    if report.is_legal() {
        lines.push(format!(
            "{path}: legal ({} cards, {} resources; {})",
            copies(&deck.main),
            copies(&deck.resources),
            deck_colors(deck, gcg_data::CardDb::embedded())
        ));
    } else {
        lines.push(format!(
            "{path}: illegal ({} violation(s))",
            report.violation_count()
        ));
    }
    if report.banlist.is_none() {
        lines.push(
            "WARNING: no Banned & Restricted list applied (--no-banlist): only the Comprehensive \
             Rules and simulator support are checked, but official and sanctioned tournaments \
             enforce the list"
                .to_owned(),
        );
    }
    lines.push(format!("format: {FORMAT_APPLIED}"));
    lines.push(list_line(args, report));
    let rules_title = if report.banlist.is_some() {
        "rules (Comprehensive Rules and Banned & Restricted list)"
    } else {
        "rules (Comprehensive Rules)"
    };
    section(&mut lines, rules_title, &report.rules);
    match &report.simulator {
        Some(v) => section(
            &mut lines,
            "simulator support (not playable by this simulator version)",
            v,
        ),
        None => lines.push(
            "simulator support: not checked (--no-implemented): the deck may hold cards this \
             simulator version cannot play"
                .to_owned(),
        ),
    }
    if let Some(ev) = &report.event {
        let title = format!("event {}: 7-day new-card rule (TRM 2.4.1)", ev.event_date);
        if ev.violations.is_empty() {
            lines.push(format!("{title}: no violations"));
        } else {
            lines.push(format!("{title}: {} violation(s)", ev.violations.len()));
            lines.extend(
                ev.violations
                    .iter()
                    .map(|v| format!("  [NEW_CARD] {}", v.message)),
            );
        }
        if !ev.unknown.is_empty() {
            lines.push(format!(
                "  not checked (the data has no release date for their set): {}",
                ev.unknown.join(", ")
            ));
        }
    }
    if !report.exceptions.is_empty() {
        lines.push("exceptions applied:".to_owned());
        for e in &report.exceptions {
            lines.push(format!(
                "  - '{}': the deck is the unmodified official {} decklist ({})",
                e.id, e.product, e.source_url
            ));
        }
    }
    if !report.hints.is_empty() {
        lines.push("hints:".to_owned());
        lines.extend(report.hints.iter().map(|h| format!("  - {}", h.text)));
    }
    if args.scope {
        scope_lines(&mut lines, args, deck);
    }
    lines
}

fn print(out: &mut dyn Write, lines: &[String]) {
    for l in lines {
        let _ = writeln!(out, "{l}");
    }
}

/// `validate` with an explicit simulator-support status (the registry in [`run`]).
///
/// # Errors
/// The deck file's error (as `gcg-sim validate` words it), a list that no data covers, or
/// `{path}: {n} violation(s)`.
pub fn run_with(
    args: &ValidateArgs,
    implemented: &dyn ImplementationStatus,
    out: &mut dyn Write,
) -> Result<(), CommandError> {
    let deck = match read_deck(&args.deck) {
        Ok(d) => d,
        Err(e) => {
            let hints = file_hints(
                gcg_data::CardDb::embedded(),
                Path::new(&args.deck.to_os_string()),
            );
            if !hints.is_empty() {
                let mut lines = vec!["hints:".to_owned()];
                lines.extend(hints.iter().map(|h| format!("  - {}", h.text)));
                print(out, &lines);
            }
            return Err(CommandError::Invalid(cli_text(&e)));
        }
    };
    let opts = LegalityOptions {
        banlist: (!args.no_banlist).then_some(ListRequest {
            as_of: args.as_of,
            region: args.region.as_deref(),
        }),
        implemented: (!args.no_implemented).then_some(implemented),
        event_date: args.event_date,
    };
    let report = check(
        &deck,
        &LegalitySources::packaged(packaged_releases()),
        &opts,
    )
    .map_err(|e| CommandError::Invalid(e.to_string()))?;
    print(out, &render(args, &deck, &report));
    if report.is_legal() {
        return Ok(());
    }
    Err(CommandError::Invalid(format!(
        "{}: {} violation(s)",
        cli_text(&CliError::Invalid(args.deck.clone())),
        report.violation_count()
    )))
}

/// `validate` over the embedded effect registry.
///
/// # Errors
/// [`run_with`]'s.
pub fn run(args: &ValidateArgs, out: &mut dyn Write) -> Result<(), CommandError> {
    run_with(args, &*Registry::embedded(), out)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use gcg_core::pool::Cancel;

    use super::*;

    const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

    fn example(name: &str) -> String {
        std::fs::read_to_string(format!("{ROOT}/examples/decks/{name}.txt")).expect("example")
    }

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gcg-eval-p6l-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn deck_file(name: &str, text: &str) -> String {
        let path = scratch().join(name);
        std::fs::write(&path, text).expect("write deck");
        path.to_string_lossy().into_owned()
    }

    /// `(status, stdout, stderr)` of `gcg-eval` with `args`.
    fn gcg_eval_cli(args: &[&str]) -> (u8, String, String) {
        let argv: Vec<OsString> = args.iter().map(OsString::from).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = crate::cli::run(&argv, None, &Cancel::new(), false, &mut out, &mut err);
        (
            status,
            String::from_utf8(out).expect("UTF-8"),
            String::from_utf8(err).expect("UTF-8"),
        )
    }

    /// `gcg-sim validate`'s body (`cli.py:129-136`) on the same file.
    fn gcg_sim_validate(path: &str) -> Result<String, String> {
        let mut out = Vec::new();
        gcg_runner::cli_support::validate_command(&PyStr::from(path), &mut out)
            .map(|()| String::from_utf8(out).expect("UTF-8"))
            .map_err(|e| cli_text(&e))
    }

    #[test]
    fn a_legal_deck_starts_with_gcg_sim_validates_line() {
        let path = deck_file("bw.txt", &example("blue-white-federation"));
        let (status, out, err) = gcg_eval_cli(&["validate", &path]);
        assert_eq!((status, err.as_str()), (0, ""));
        let sim = gcg_sim_validate(&path).expect("legal");
        assert_eq!(out.lines().next(), sim.lines().next());
        assert!(sim.ends_with(": legal (50 cards, 10 resources; Blue/White)\n"));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[1..],
            [
                "format: Constructed",
                "banned & restricted list: effective 2026-09-25 (the current list; regions \
                 asia-en, en)",
                "rules (Comprehensive Rules and Banned & Restricted list): no violations",
                "simulator support (not playable by this simulator version): no violations",
            ]
        );
    }

    /// W-75: `--no-banlist` accepts 4 copies of the banned GD01-020 and the header warns; the
    /// default run reports BANNED and exits 1, as `gcg-sim validate` does.
    #[test]
    fn w75_no_banlist_warns_in_the_header() {
        let text = example("blue-white-federation").replace("4 GD01-008 Guntank", "4 GD01-020");
        let path = deck_file("banned.txt", &text);
        let (status, out, err) = gcg_eval_cli(&["validate", &path, "--no-banlist"]);
        assert_eq!(status, 0, "{err}");
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].ends_with(": legal (50 cards, 10 resources; Blue/White)"));
        assert!(lines[1].starts_with(
            "WARNING: no Banned & Restricted list applied (--no-banlist): only the \
             Comprehensive Rules"
        ));
        assert_eq!(lines[3], "banned & restricted list: none (--no-banlist)");
        assert_eq!(lines[4], "rules (Comprehensive Rules): no violations");
        let (status, out, err) = gcg_eval_cli(&["validate", &path]);
        assert_eq!(status, 1);
        assert!(
            out.contains("  [BANNED] GD01-020 Anksha is banned"),
            "{out}"
        );
        assert!(!out.contains("WARNING"));
        assert_eq!(err, format!("error: {path}: 1 violation(s)\n"));
        let sim = gcg_sim_validate(&path).unwrap_err();
        assert!(
            sim.starts_with(&format!("{path}: 1 violation(s)\n  [BANNED]")),
            "{sim}"
        );
    }

    /// W-76: a card released 3 days before `--event-date` fails the optional check and passes
    /// without it.
    #[test]
    fn w76_event_date_fails_a_three_day_old_card() {
        let text =
            example("blue-white-federation").replace("2 GD04-122 Jaburo", "2 ST11-016 Mad Angler");
        let path = deck_file("new-card.txt", &text);
        let (status, _, err) = gcg_eval_cli(&["validate", &path]);
        assert_eq!((status, err.as_str()), (0, ""));
        let (status, out, err) = gcg_eval_cli(&["validate", &path, "--event-date", "2026-09-28"]);
        assert_eq!(status, 1);
        assert_eq!(err, format!("error: {path}: 1 violation(s)\n"));
        assert!(out.contains(
            "event 2026-09-28: 7-day new-card rule (TRM 2.4.1): 1 violation(s)\n  [NEW_CARD] \
             ST11-016 Mad Angler was first released on 2026-09-25 (Aquatic Assault [ST11]), 3 \
             day(s) before the event on 2026-09-28;"
        ));
        assert!(out.contains("  not checked (the data has no release date for their set): R-001"));
        let (status, out, _) = gcg_eval_cli(&["validate", &path, "--event-date", "2026-10-02"]);
        assert_eq!(status, 0);
        assert!(out.contains("event 2026-10-02: 7-day new-card rule (TRM 2.4.1): no violations"));
    }

    /// W-76: `--scope` lists the unchecked rules, flags RP-001 (Edition Beta only), and names
    /// Constructed as the applied format with Sealed and Draft unsupported.
    #[test]
    fn w76_scope_lists_unchecked_rules_and_flags_edition_beta() {
        let text =
            example("blue-white-federation").replace("10 R-001 Resource", "9 R-001\n1 RP-001");
        let path = deck_file("beta.txt", &text);
        let (status, out, _) = gcg_eval_cli(&["validate", &path, "--scope"]);
        assert_eq!(status, 0);
        for want in [
            "scope:",
            "  format applied: Constructed (standard constructed (official and sanctioned \
             tournaments); the Banned & Restricted list applies to the deck and sideboard \
             combined",
            "    - Sealed: Players will receive specific event product",
            "    - Draft: Players will receive booster packs",
            "    - 7-day new-card rule for Premier events (TRM 2.4.1; Sanctioned Floor Rules \
             Section 5)",
            "    - physical card language, condition, sleeves, deck lists and deck checks",
            "    (the 7-day new-card rule is checked only with --event-date: not checked in this \
             run)",
            "    - intentional_draws_concessions_penalties: Not modelled.",
            "Card numbers printed only in Edition Beta: EXBP-001, RP-001",
            "  FLAG: this deck uses Edition Beta-only card numbers, whose tournament legality is \
             undecided: RP-001",
        ] {
            assert!(out.contains(want), "{want:?} missing from\n{out}");
        }
        let (_, plain, _) = gcg_eval_cli(&["validate", &path]);
        assert!(!plain.contains("scope:"));
    }

    /// W-72: `--as-of`, `--region` and their errors.
    #[test]
    fn w72_as_of_and_region() {
        let path = deck_file("bw-dates.txt", &example("blue-white-federation"));
        let (status, out, _) =
            gcg_eval_cli(&["validate", &path, "--as-of", "2026-10-01", "--region", "en"]);
        assert_eq!(status, 0);
        assert!(out.contains(
            "banned & restricted list: effective 2026-09-25 (the current list, in effect on \
             2026-10-01, region en; regions asia-en, en)"
        ));
        let (status, out, err) = gcg_eval_cli(&["validate", &path, "--as-of", "2026-09-24"]);
        assert_eq!((status, out.as_str()), (1, ""));
        assert!(err.starts_with(
            "error: no Banned & Restricted list in the data was in effect on 2026-09-24;"
        ));
        let (status, _, err) = gcg_eval_cli(&["validate", &path, "--as-of", "2026-9-24"]);
        assert_eq!(status, 2);
        assert!(err.ends_with(
            "error: argument --as-of: invalid date value: \"2026-9-24\" (expected YYYY-MM-DD)\n"
        ));
        let (status, _, err) = gcg_eval_cli(&["validate", &path, "--region", "us"]);
        assert_eq!(status, 2);
        assert!(
            err.contains("argument --region: invalid choice: 'us'"),
            "{err}"
        );
        let (status, _, err) =
            gcg_eval_cli(&["validate", &path, "--no-banlist", "--as-of", "2026-10-01"]);
        assert_eq!(status, 2);
        assert!(err.ends_with("error: argument --no-banlist: not allowed with argument --as-of\n"));
    }

    /// W-77: a `#` after a card prints its hint and `gcg-sim validate`'s error; an EX Resource
    /// gets the token hint under its CARD_TYPE violation.
    #[test]
    fn w77_hints_on_the_command_line() {
        let text = example("blue-white-federation")
            .replace("4 GD01-008 Guntank", "4 GD01-008 Guntank # early");
        let path = deck_file("hash.txt", &text);
        let (status, out, err) = gcg_eval_cli(&["validate", &path]);
        assert_eq!(status, 1);
        let sim = gcg_sim_validate(&path).unwrap_err();
        assert_eq!(err, format!("error: {sim}\n"));
        assert!(err.contains("[NAME_MISMATCH]") || err.contains("is 'Guntank' in the card data"));
        assert!(
            out.starts_with("hints:\n  - line 7: 'Guntank' matches GD01-008"),
            "{out}"
        );
        let text =
            example("blue-white-federation").replace("10 R-001 Resource", "9 R-001\n1 EXR-001");
        let path = deck_file("exr.txt", &text);
        let (status, out, _) = gcg_eval_cli(&["validate", &path]);
        assert_eq!(status, 1);
        assert!(out.contains("  [CARD_TYPE] EXR-001 EX Resource is a token (EX RESOURCE);"));
        assert!(out.contains("hints:\n  - line "), "{out}");
        assert!(out.contains("EXR-001 is the EX Resource token"));
    }

    /// W-78: a synthetic unimplemented card is listed under simulator support, and the deck
    /// passes the rules with `--no-implemented`.
    #[test]
    fn w78_simulator_support_heading() {
        struct NoGuntank(gcg_data::DefId);
        impl ImplementationStatus for NoGuntank {
            fn unimplemented_error(&self, def: gcg_data::DefId) -> Option<&str> {
                (def == self.0).then_some("synthetic: not compiled")
            }
        }
        let status = NoGuntank(
            gcg_data::CardDb::embedded()
                .get("GD01-008")
                .expect("card")
                .def_id,
        );
        let path = deck_file("unimpl.txt", &example("blue-white-federation"));
        let mut a = ValidateArgs {
            deck: PyStr::from(path.as_str()),
            as_of: None,
            region: None,
            no_banlist: false,
            no_implemented: false,
            event_date: None,
            scope: false,
        };
        let mut out = Vec::new();
        let r = run_with(&a, &status, &mut out);
        assert_eq!(
            r,
            Err(CommandError::Invalid(format!("{path}: 1 violation(s)")))
        );
        let text = String::from_utf8(out).expect("UTF-8");
        assert!(text.contains(
            "rules (Comprehensive Rules and Banned & Restricted list): no violations\nsimulator \
             support (not playable by this simulator version): 1 violation(s)\n  [UNIMPLEMENTED] \
             GD01-008 Guntank has no implemented effect"
        ), "{text}");
        a.no_implemented = true;
        let mut out = Vec::new();
        assert_eq!(run_with(&a, &status, &mut out), Ok(()));
        let text = String::from_utf8(out).expect("UTF-8");
        assert!(text.contains("simulator support: not checked (--no-implemented)"));
        let (status, _, _) = gcg_eval_cli(&["validate", &path, "--no-implemented"]);
        assert_eq!(status, 0);
    }

    /// W-144: a 5,000-digit count gives a SYNTAX violation and exit 1 in both `gcg-sim
    /// validate`'s body and `gcg-eval validate`.
    #[test]
    fn w144_huge_count_exits_one_with_syntax() {
        let path = deck_file(
            "big.txt",
            &format!("{} GD01-001\n10 R-001\n", "9".repeat(5_000)),
        );
        let want = format!(
            "{path}: deck 'big' has 1 problem(s):\n  - line 1: count must be at most \
             18446744073709551615 for GD01-001"
        );
        assert_eq!(gcg_sim_validate(&path), Err(want.clone()));
        let (status, out, err) = gcg_eval_cli(&["validate", &path]);
        assert_eq!((status, out.as_str()), (1, ""));
        assert_eq!(err, format!("error: {want}\n"));
        let path = deck_file("huge.txt", "1000000000000 GD01-001\n10 R-001\n");
        let (status, out, _) = gcg_eval_cli(&["validate", &path]);
        assert_eq!(status, 1);
        assert!(out.contains(
            "  [MAIN_SIZE] the deck has 1000000000000 cards; it must have exactly 50 (rule 6-1-1)"
        ));
    }

    /// The embedded sources are the packaged files, and the `analysis.json` scope block (for
    /// `gcg-eval benchmark --analysis`, W-75/W-76) names the format, the list date and the
    /// unmodelled tournament procedures.
    #[test]
    fn embedded_sources_and_the_analysis_scope_block() {
        let read = |rel: &str| std::fs::read_to_string(format!("{ROOT}/{rel}")).expect(rel);
        assert_eq!(PRODUCTS_JSON, read("src/gcg_sim/data/gcgapi/products.json"));
        assert_eq!(
            EDITION_LANGUAGE_JSON,
            read("src/gcg_sim/data/official/edition_language.json")
        );
        let gd06 = packaged_releases().first_release("GD06").expect("GD06");
        assert_eq!(gd06.1.to_string(), "2026-10-30");
        let current = gcg_data::legality::BanlistHistory::packaged().current();
        let block = packaged_scope().to_json(Some(current));
        let text =
            gcg_core::pyjson::dumps(&block, gcg_core::pyjson::DumpOptions::COMPACT_SORTED_ASCII);
        for want in [
            "\"format\":\"Constructed\"",
            "\"effective_date\":\"2026-09-25\"",
            "\"intentional_draws_concessions_penalties\":\"Not modelled.\"",
            "\"name\":\"Sealed\"",
            "\"name\":\"Draft\"",
        ] {
            assert!(text.contains(want), "{want} missing from {text}");
        }
    }

    #[test]
    fn help_lists_the_options() {
        let (status, out, _) = gcg_eval_cli(&["validate", "--help"]);
        assert_eq!(status, 0);
        for o in [
            "--as-of",
            "--region",
            "--no-banlist",
            "--no-implemented",
            "--event-date",
            "--scope",
            "{asia-en,en}",
        ] {
            assert!(out.contains(o), "{o} missing from {out}");
        }
        let (_, top, _) = gcg_eval_cli(&["--help"]);
        assert!(top.contains("validate"));
    }
}
