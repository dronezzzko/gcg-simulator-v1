//! `summary.md` (`reports/summary.py`): a human-readable view of `results.json`, rendered from
//! the results document alone (no wall-clock data).
//!
//! The document has been validated against the schema before it is rendered, so every key the
//! renderer reads exists with the schema's type. A missing key reads as `None`, as
//! `dict.get` would give, instead of Python's `KeyError`.

use gcg_core::pyfloat::{format_fixed, repr_f64};
use gcg_core::pyjson::{DumpOptions, Json, dumps};

/// `TOP_BENCH_CARDS` (`reports/summary.py:8`).
pub const TOP_BENCH_CARDS: usize = 10;

/// `OUTCOME_HEADER` (`reports/summary.py:40`).
const OUTCOME_HEADER: [&str; 6] = [
    "",
    "n",
    "DUT wins",
    "Bench wins",
    "Draws",
    "DUT win rate [95% CI]",
];

static NULL: Json = Json::Null;

/// `res[key]` (the value, or `None` when absent).
fn get<'a>(j: &'a Json, key: &str) -> &'a Json {
    j.get(key).unwrap_or(&NULL)
}

/// `res[k1][k2]...`.
fn path<'a>(j: &'a Json, keys: &[&str]) -> &'a Json {
    keys.iter().fold(j, |at, k| get(at, k))
}

fn items(j: &Json) -> &[Json] {
    match j {
        Json::Array(v) => v,
        _ => &[],
    }
}

/// Python `str(value)` of a JSON scalar: `None`, `True`/`False`, the int digits, the float
/// `repr`, the string itself. Containers never reach `str()` in a valid document; they render
/// as their compact JSON text.
fn py_str(j: &Json) -> String {
    match j {
        Json::Null => "None".to_owned(),
        Json::Bool(true) => "True".to_owned(),
        Json::Bool(false) => "False".to_owned(),
        Json::Int(i) => i.to_string(),
        Json::Float(x) => repr_f64(*x),
        Json::Str(s) => s.clone(),
        Json::Array(_) | Json::Object(_) => dumps(
            j,
            DumpOptions {
                indent: None,
                sort_keys: false,
                compact: false,
                ensure_ascii: false,
            },
        ),
    }
}

/// The value as a float (`int` or `float`), `None` for `None` or a non-number.
#[expect(
    clippy::cast_precision_loss,
    reason = "Python converts an int to float the same way in 100 * x and format()"
)]
const fn number(j: &Json) -> Option<f64> {
    match j {
        Json::Int(i) => Some(*i as f64),
        Json::Float(x) => Some(*x),
        _ => None,
    }
}

/// `pct(x)` (`reports/summary.py:11-12`): `"n/a"` or `f"{100 * x:.1f}%"`.
fn pct(x: &Json) -> String {
    number(x).map_or_else(
        || "n/a".to_owned(),
        |v| format!("{}%", format_fixed(100.0 * v, 1)),
    )
}

/// `num(x, digits)` (`reports/summary.py:15-16`): `"-"` or `f"{x:.{digits}f}"`.
fn num(x: &Json, digits: usize) -> String {
    number(x).map_or_else(|| "-".to_owned(), |v| format_fixed(v, digits))
}

/// `points(x)` (`reports/summary.py:19-20`): `"-"` or `f"{100 * x:+.1f} pp"`.
fn points(x: &Json) -> String {
    number(x).map_or_else(|| "-".to_owned(), |v| format!("{:+.1} pp", 100.0 * v))
}

/// `rate_text(r)` (`reports/summary.py:23-27`).
fn rate_text(r: &Json) -> String {
    if matches!(get(r, "rate"), Json::Null) {
        return "n/a (n=0)".to_owned();
    }
    let ci = items(get(r, "ci95"));
    let lo = ci.first().unwrap_or(&NULL);
    let hi = ci.get(1).unwrap_or(&NULL);
    format!(
        "{} [{}, {}] (n={})",
        pct(get(r, "rate")),
        pct(lo),
        pct(hi),
        py_str(get(r, "n"))
    )
}

/// `_table(header, rows)` (`reports/summary.py:30-33`).
fn table(header: &[&str], rows: &[Vec<String>]) -> Vec<String> {
    let mut lines = vec![
        format!("| {} |", header.join(" | ")),
        format!("|{}", "---|".repeat(header.len())),
    ];
    lines.extend(rows.iter().map(|row| format!("| {} |", row.join(" | "))));
    lines
}

/// `_outcome_row(label, o)` (`reports/summary.py:36-37`).
fn outcome_row(label: &str, o: &Json) -> Vec<String> {
    vec![
        label.to_owned(),
        py_str(get(o, "n")),
        py_str(get(o, "dut_wins")),
        py_str(get(o, "bench_wins")),
        py_str(get(o, "draws")),
        rate_text(get(o, "dut_win_rate")),
    ]
}

/// Python `str.capitalize()`: the first character upper-cased, the rest lower-cased
/// (`reports/summary.py:177`; the schema fixes the value to `"holm"`).
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |first| {
        first
            .to_uppercase()
            .chain(chars.as_str().to_lowercase().chars())
            .collect()
    })
}

/// The "search" line of results 1.2.0 (PLAN §12.7): the strengthened search's preset and its
/// mapping, budget, enabled mechanisms and weights; absent from 1.1.0 documents.
fn search_line(ai: &Json) -> Option<String> {
    let s = ai.get("search")?;
    let b = get(s, "budget");
    let mechanisms: Vec<String> = items(get(s, "mechanisms"))
        .iter()
        .map(|m| format!("`{}`", py_str(m)))
        .collect();
    Some(format!(
        "Search: `{}`, preset `{}` ({} mapping), {} iterations per option (min {}, max {}, \
         auxiliary apply cap {} per iteration), mechanisms: {}, weights `{}`.",
        py_str(get(s, "algorithm")),
        py_str(get(s, "preset")),
        py_str(get(s, "mapping")),
        py_str(get(b, "iterations_per_option")),
        py_str(get(b, "min_iterations")),
        py_str(get(b, "max_iterations")),
        py_str(get(b, "aux_apply_cap")),
        if mechanisms.is_empty() {
            "none".to_owned()
        } else {
            mechanisms.join(", ")
        },
        py_str(get(s, "weights")),
    ))
}

/// `_headline(res, command)` (`reports/summary.py:43-55`), plus the search line of results
/// 1.2.0.
fn headline(res: &Json, command: &str) -> Vec<String> {
    let (d, c, ai) = (get(res, "decks"), get(res, "config"), get(res, "ai"));
    let dut = py_str(path(d, &["deck_under_test", "name"]));
    let bench = py_str(path(d, &["benchmark", "name"]));
    let mut lines = vec![
        format!("# {dut} vs {bench}"),
        String::new(),
        format!(
            "Deck under test (DUT) **{dut}** against benchmark **{bench}**: {} {} matches, master \
             seed {}, AI preset `{}` (agents `{}` / `{}`, factory `{}`).",
            py_str(get(c, "matches")),
            py_str(get(c, "format")).to_uppercase(),
            py_str(path(res, &["seeds", "master"])),
            py_str(get(ai, "preset")),
            py_str(get(ai, "dut_agent")),
            py_str(get(ai, "bench_agent")),
            py_str(get(ai, "factory")),
        ),
        String::new(),
    ];
    if let Some(line) = search_line(ai) {
        lines.push(line);
        lines.push(String::new());
    }
    lines.push(format!("Reproduce: `{command}`"));
    lines.push(String::new());
    lines
}

/// `_results(res)` (`reports/summary.py:58-75`).
fn results(res: &Json) -> Vec<String> {
    let (r, s) = (get(res, "results"), get(res, "splits"));
    let rows = [
        outcome_row("Matches", get(r, "matches")),
        outcome_row("Games", get(r, "games")),
    ];
    let mut split_rows = vec![
        outcome_row("On the play", get(s, "on_play")),
        outcome_row("On the draw", get(s, "on_draw")),
    ];
    if let Json::Object(by) = get(s, "by_game_number") {
        let mut keyed: Vec<(String, &Json)> = by
            .iter()
            .map(|(k, v)| {
                let key = match k {
                    gcg_core::pyjson::Key::Str(s) => s.clone(),
                    gcg_core::pyjson::Key::Int(i) => i.to_string(),
                };
                (key, v)
            })
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        split_rows.extend(
            keyed
                .iter()
                .map(|(k, v)| outcome_row(&format!("Game {k}"), v)),
        );
    }
    let mut lines = vec!["## Result".to_owned(), String::new()];
    lines.extend(table(&OUTCOME_HEADER, &rows));
    lines.extend(["".to_owned(), "## Splits".to_owned(), String::new()]);
    lines.extend(table(&OUTCOME_HEADER, &split_rows));
    lines.push(String::new());
    lines
}

/// `_endings(res)` (`reports/summary.py:78-96`).
fn endings(res: &Json) -> Vec<String> {
    let t = path(res, &["game_length", "turns"]);
    let dr = get(res, "draws");
    let by = path(res, &["game_length", "by_result"]);
    let rows: Vec<Vec<String>> = items(get(res, "end_reasons"))
        .iter()
        .map(|e| outcome_row(&py_str(get(e, "reason")), e))
        .collect();
    let mut header = vec!["Reason"];
    header.extend(&OUTCOME_HEADER[1..]);
    let mut lines = vec!["## How games end".to_owned(), String::new()];
    lines.extend(table(&header, &rows));
    lines.extend([
        String::new(),
        format!(
            "Game length (turns): mean {}, median {}, p10 {}, p90 {}, range {}-{}. Median by DUT \
             result: win {}, loss {}, draw {}.",
            num(get(t, "mean"), 1),
            py_str(get(t, "median")),
            py_str(get(t, "p10")),
            py_str(get(t, "p90")),
            py_str(get(t, "min")),
            py_str(get(t, "max")),
            num(path(by, &["win", "median"]), 0),
            num(path(by, &["loss", "median"]), 0),
            num(path(by, &["draw", "median"]), 0),
        ),
        String::new(),
        format!(
            "Draws: {} ({} simultaneous defeat, {} turn limit); {} scored for BO3 by the \
             turn-player-loses rule (TRM 5.2).",
            py_str(get(dr, "games")),
            py_str(get(dr, "both_defeated")),
            py_str(get(dr, "turn_limit")),
            py_str(get(dr, "scored_for_match_by_turn_player_rule")),
        ),
        String::new(),
    ]);
    lines
}

/// `_mulligan(res)` (`reports/summary.py:99-109`).
fn mulligan(res: &Json) -> Vec<String> {
    let m = get(res, "mulligan");
    vec![
        "## Redraws".to_owned(),
        String::new(),
        format!(
            "DUT redraw rate {}; DUT win rate after a redraw {}, after keeping {}. Benchmark \
             redraw rate {}.",
            rate_text(path(m, &["dut", "redraw_rate"])),
            rate_text(path(m, &["dut", "win_rate_after_redraw"])),
            rate_text(path(m, &["dut", "win_rate_after_keep"])),
            rate_text(path(m, &["bench", "redraw_rate"])),
        ),
        String::new(),
    ]
}

/// `_card_row(c)` (`reports/summary.py:112-125`).
fn card_row(c: &Json) -> Vec<String> {
    let (played, held) = (get(c, "played"), get(c, "held_never_played"));
    vec![
        format!(
            "{} {}",
            py_str(get(c, "card_number")),
            py_str(get(c, "name"))
        ),
        py_str(get(c, "copies")),
        format!(
            "{} ({})",
            py_str(path(c, &["drawn", "games"])),
            pct(path(c, &["drawn", "game_rate"]))
        ),
        py_str(get(played, "games")),
        num(path(played, &["own_turn", "mean"]), 1),
        rate_text(get(c, "win_rate_when_drawn")),
        rate_text(get(c, "win_rate_when_not_drawn")),
        rate_text(get(c, "win_rate_when_played")),
        format!(
            "{} ({})",
            py_str(get(held, "copies")),
            pct(get(held, "rate_of_drawn_copies"))
        ),
        pct(path(c, &["mulligan", "rate"])),
    ]
}

/// `_cards(res)` (`reports/summary.py:128-146`).
fn cards(res: &Json) -> Vec<String> {
    let header = [
        "Card",
        "Copies",
        "Drawn games",
        "Played games",
        "Mean own turn played",
        "Win rate when drawn",
        "Win rate when not drawn",
        "Win rate when played",
        "Drawn, never played",
        "Redrawn when in initial hand",
    ];
    let rows: Vec<Vec<String>> = items(get(res, "cards")).iter().map(card_row).collect();
    let mut lines = vec!["## Deck-under-test cards".to_owned(), String::new()];
    lines.extend(table(&header, &rows));
    lines.push(String::new());
    lines
}

/// `_bench(res)` (`reports/summary.py:149-166`).
fn bench(res: &Json) -> Vec<String> {
    let header = [
        "Benchmark card",
        "Seen games",
        "DUT loss rate when seen",
        "when not seen",
        "Lift",
    ];
    let rows: Vec<Vec<String>> = items(get(res, "benchmark_cards"))
        .iter()
        .filter(|b| matches!(get(b, "rank_eligible"), Json::Bool(true)))
        .take(TOP_BENCH_CARDS)
        .map(|b| {
            vec![
                format!(
                    "{} {}",
                    py_str(get(b, "card_number")),
                    py_str(get(b, "name"))
                ),
                py_str(get(b, "seen_games")),
                rate_text(get(b, "dut_loss_rate_when_seen")),
                rate_text(get(b, "dut_loss_rate_when_not_seen")),
                points(get(b, "loss_rate_lift")),
            ]
        })
        .collect();
    let mut lines = vec![
        "## Benchmark cards most associated with DUT losses".to_owned(),
        String::new(),
    ];
    if rows.is_empty() {
        let need = py_str(path(res, &["hypothesis_thresholds", "min_games"]));
        lines.extend([
            format!("No benchmark card has {need}+ games both seen and unseen yet."),
            String::new(),
        ]);
        return lines;
    }
    lines.extend(table(&header, &rows));
    lines.push(String::new());
    lines
}

/// `_evidence_note(h)` (`reports/summary.py:191-198`).
fn evidence_note(h: &Json) -> String {
    let ev = get(h, "evidence");
    let sample = py_str(get(h, "sample_size"));
    match ev.get("p_value") {
        None => format!("(sample: {sample})"),
        Some(p) => format!(
            "(sample: {sample}; p = {}, {} comparison(s) of this kind)",
            py_str(p),
            ev.get("comparisons").map_or_else(|| "1".to_owned(), py_str)
        ),
    }
}

/// `_hypotheses(res)` (`reports/summary.py:169-188`).
fn hypotheses(res: &Json) -> Vec<String> {
    let mut lines = vec![
        "## Tuning hypotheses (to test, not conclusions)".to_owned(),
        String::new(),
    ];
    let t = get(res, "hypothesis_thresholds");
    let gate = format!(
        "a two-proportion z-test significant at {} after a {} correction per kind of \
         comparison, with at least {} games per side (10 for redraws)",
        py_str(get(t, "alpha")),
        capitalize(&py_str(get(t, "multiple_comparisons"))),
        py_str(get(t, "min_games")),
    );
    let found = items(get(res, "hypotheses"));
    if found.is_empty() {
        lines.extend([
            format!("No signal cleared the noise gate ({gate}). Run more games."),
            String::new(),
        ]);
        return lines;
    }
    lines.extend([
        format!(
            "Comparison signals (win-rate differences) cleared the noise gate: {gate}. 'Never \
             played' signals are descriptive."
        ),
        String::new(),
    ]);
    lines.extend(
        found
            .iter()
            .map(|h| format!("- {} {}", py_str(get(h, "statement")), evidence_note(h))),
    );
    lines.push(String::new());
    lines
}

/// `_provenance(res)` (`reports/summary.py:201-227`).
fn provenance(res: &Json) -> Vec<String> {
    let v = get(res, "versions");
    let mut lines = vec![
        "## Versions".to_owned(),
        String::new(),
        format!(
            "gcg-sim {}; results schema {}; card data {} (built {}); Comprehensive Rules {} ({}); \
             Banned & Restricted list {}; BO3 rules {}.",
            py_str(get(v, "package")),
            py_str(get(v, "results_schema")),
            py_str(get(v, "dataset_version")),
            py_str(get(v, "data_built_at")),
            py_str(get(v, "rules_version")),
            py_str(get(v, "rules_effective_date")),
            py_str(get(v, "banlist_effective_date")),
            py_str(get(v, "bo3_rules_effective_date")),
        ),
        String::new(),
        "## Conflict resolutions affecting these decks".to_owned(),
        String::new(),
    ];
    let conflicts: Vec<String> = items(get(res, "conflict_resolutions"))
        .iter()
        .map(|c| {
            let decks: Vec<String> = items(get(c, "decks")).iter().map(py_str).collect();
            format!(
                "- `{}` ({} {}; {}): {}",
                py_str(get(c, "conflict_id")),
                py_str(get(c, "card_number")),
                py_str(get(c, "name")),
                decks.join(", "),
                py_str(get(c, "decision"))
            )
        })
        .collect();
    or_none(&mut lines, conflicts);
    lines.extend([String::new(), "## Replays".to_owned(), String::new()]);
    let replays: Vec<String> = items(get(res, "replays"))
        .iter()
        .map(|r| {
            format!(
                "- `{}`: {}, match {} game {}, {} turns, {}",
                py_str(get(r, "file")),
                py_str(get(r, "result")),
                py_str(get(r, "match_index")),
                py_str(get(r, "game_number")),
                py_str(get(r, "turns")),
                py_str(get(r, "end_reason"))
            )
        })
        .collect();
    or_none(&mut lines, replays);
    lines.extend([String::new(), "## Notes".to_owned(), String::new()]);
    lines.extend(
        items(get(res, "notes"))
            .iter()
            .map(|n| format!("- {}", py_str(n))),
    );
    lines.push(String::new());
    lines
}

/// `[...] or ["- none"]`.
fn or_none(lines: &mut Vec<String>, entries: Vec<String>) {
    if entries.is_empty() {
        lines.push("- none".to_owned());
    } else {
        lines.extend(entries);
    }
}

/// `render_summary(res, command)` (`reports/summary.py:230-241`): the sections joined with
/// `"\n"`; each ends with an empty line, so the text ends with exactly one newline.
#[must_use]
pub fn render_summary(results: &Json, command: &str) -> String {
    let sections = [
        headline(results, command),
        self::results(results),
        endings(results),
        mulligan(results),
        cards(results),
        bench(results),
        hypotheses(results),
        provenance(results),
    ];
    sections.concat().join("\n")
}
