//! `gcg-eval transcripts OUT_DIR KEY_FILE --run RUN_DIR [--run RUN_DIR ...]` (W-119, used by
//! W-17's audit): `scripts/blind_transcripts.py`'s blinded transcripts of saved games, over the
//! public replay API (`gcg_runner::replay::check_replay`, `describe_action`) and the engine's
//! public view.
//!
//! Each `replays/*.json` under a run directory (at any depth, in `sorted(run.rglob(...))`
//! order) must reproduce its recorded outcome; it is then re-played from its seed and actions,
//! and every decision with more than one option is printed with the full board (both hands,
//! the non-decider's marked hidden from the decider), every legal option and the move played
//! (`blind_transcripts.py:113-136`). Search statistics, agent names, agent seeds, decision logs
//! and versions are left out. A file is named by [`game_id`], a hash of the game alone (seed,
//! chooser, caps, both decklists and the actions), so auditors cannot tell which AI played and
//! the same game gets the same id wherever its replay file lives; the key file maps each id to
//! every file that holds the game.
//!
//! The run directories are a repeatable `--run` option, because the shared argparse implementation has
//! no `nargs='+'` positional; the rest is the script's command line.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use gcg_core::pyjson::{DumpOptions, Json, Key, dumps};
use gcg_data::{CardType, py_path_str, py_repr};
use gcg_effects::{Kw, Registry};
use gcg_engine::view::{ap_of, derived, hp_of, is_linked, keywords};
use gcg_engine::{
    Action, ActionKind, DecisionView, GameState, NewGameOptions, Prompt, Uid, Zone, apply,
    new_game_by_number,
};
use gcg_runner::argparse::{Action as Arg, Converter, Namespace, Parser, PyStr, Value};
use gcg_runner::replay::{check_replay, describe_action};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::bench_cmd::CommandError;

/// The replay fields that define the game (`blind_transcripts.py:39`); agents, agent seeds,
/// logs and labels stay out.
pub const GAME_FIELDS: [&str; 5] = ["seed", "chooser", "turn_limit", "max_actions", "actions"];

/// Words of the decision log and the agent descriptions that a blinded transcript never holds
/// (`blind_transcripts.py:163`).
pub const LEAKS: [&str; 8] = [
    "visits",
    "iterations",
    "mean",
    "alternatives",
    "mcts",
    "search",
    "greedy",
    "agent",
];

/// The parsed `transcripts` arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TranscriptsArgs {
    pub out: PathBuf,
    pub key_file: PathBuf,
    pub runs: Vec<PathBuf>,
}

const DESCRIPTION: &str = "Blinded transcripts of saved games for a replay audit of the AI \
     (scripts/blind_transcripts.py, W-119): no search statistics, agent names or versions, and \
     file ids from the game alone.";

/// The `transcripts` subcommand parser.
#[must_use]
pub fn parser(prog: &str) -> Parser {
    let mut p = Parser::new(&format!("{prog} transcripts"), Some(DESCRIPTION));
    p.push(Arg::positional(
        "out_dir",
        Converter::Path,
        Some("directory of the game-<id>.txt transcripts".to_owned()),
    ));
    p.push(Arg::positional(
        "key_file",
        Converter::Path,
        Some("JSON file mapping each id to its replay files".to_owned()),
    ));
    let mut run = Arg::append(
        "--run",
        "run",
        Converter::Path,
        None,
        "a run directory whose replays/*.json files (at any depth) are transcribed \
         (repeatable)"
            .to_owned(),
    );
    run.required = true;
    p.push(run);
    p
}

fn path_of(ns: &Namespace, dest: &str) -> PathBuf {
    match ns.get(dest) {
        Some(Value::Path(s)) => PathBuf::from(s.to_os_string()),
        _ => unreachable!("{dest} is a Path argument"),
    }
}

/// The typed arguments of a parsed `transcripts` namespace.
#[must_use]
pub fn args(ns: &Namespace) -> TranscriptsArgs {
    let runs = match ns.get("run") {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Path(s) => Some(PathBuf::from(s.to_os_string())),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    TranscriptsArgs {
        out: path_of(ns, "out_dir"),
        key_file: path_of(ns, "key_file"),
        runs,
    }
}

/// A `json.loads` value as the JSON writer's value (object keys in document order).
fn to_json(v: &JsonValue) -> Json {
    match v {
        JsonValue::Null => Json::Null,
        JsonValue::Bool(b) => Json::Bool(*b),
        JsonValue::Number(n) => n
            .as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from))
            .map_or_else(|| Json::Float(n.as_f64().unwrap_or(f64::NAN)), Json::Int),
        JsonValue::String(s) => Json::Str(s.clone()),
        JsonValue::Array(items) => Json::Array(items.iter().map(to_json).collect()),
        JsonValue::Object(map) => Json::Object(
            map.iter()
                .map(|(k, v)| (Key::Str(k.clone()), to_json(v)))
                .collect(),
        ),
    }
}

fn field<'a>(doc: &'a JsonValue, key: &str) -> Result<&'a JsonValue, String> {
    doc.get(key)
        .ok_or_else(|| format!("the replay has no {key:?}"))
}

/// `int(s["seat"])` of a seat that `check_replay` accepted.
fn seat_number(seat: &JsonValue) -> i64 {
    seat.get("seat")
        .and_then(JsonValue::as_i64)
        .unwrap_or(i64::MAX)
}

/// The seats sorted by `int(s["seat"])` (a stable sort, `blind_transcripts.py:59`).
fn sorted_seats(doc: &JsonValue) -> Result<Vec<&JsonValue>, String> {
    let mut seats: Vec<&JsonValue> = field(doc, "seats")?
        .as_array()
        .ok_or("the replay's seats are not a list")?
        .iter()
        .collect();
    seats.sort_by_key(|s| seat_number(s));
    Ok(seats)
}

/// `game_id(doc)` (`blind_transcripts.py:57-64`): the first 12 hex digits of the SHA-256 of
/// `json.dumps(content, sort_keys=True, separators=(",", ":"), ensure_ascii=True)`, where
/// `content` is the game fields and both seats' `main` and `resources`, by seat.
///
/// # Errors
/// A field is missing.
pub fn game_id(doc: &JsonValue) -> Result<String, String> {
    let mut content = Json::object();
    for k in GAME_FIELDS {
        content.set(k, to_json(field(doc, k)?));
    }
    let decks: Vec<Json> = sorted_seats(doc)?
        .into_iter()
        .map(|s| -> Result<Json, String> {
            let mut d = Json::object();
            d.set("main", to_json(field(s, "main")?));
            d.set("resources", to_json(field(s, "resources")?));
            Ok(d)
        })
        .collect::<Result<_, _>>()?;
    content.set("decks", Json::Array(decks));
    let text = dumps(&content, DumpOptions::COMPACT_SORTED_ASCII);
    let hex: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(hex[..12].to_owned())
}

/// `decklist(seat)` (`blind_transcripts.py:46-54`): `main` and `resources` expanded
/// (`range(int(c))`, so a negative count gives no copies) and sorted.
fn decklist(seat: &JsonValue) -> Result<[Vec<String>; 2], String> {
    let mut out: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    for (side, key) in out.iter_mut().zip(["main", "resources"]) {
        let counts = field(seat, key)?
            .as_object()
            .ok_or_else(|| format!("the seat's {key} is not an object"))?;
        for (number, count) in counts {
            let copies = count
                .as_i64()
                .map(|c| usize::try_from(c.max(0)).unwrap_or(usize::MAX))
                .filter(|&c| u128::try_from(c).is_ok_and(|c| c <= gcg_data::MAX_EXPANDED_CARDS))
                .ok_or_else(|| format!("the count of {number} is not a card count"))?;
            side.extend(std::iter::repeat_n(number.clone(), copies));
        }
        side.sort();
    }
    Ok(out)
}

fn int_field<T: TryFrom<i64>>(doc: &JsonValue, key: &str) -> Result<T, String> {
    field(doc, key)?
        .as_i64()
        .and_then(|v| T::try_from(v).ok())
        .ok_or_else(|| format!("the replay's {key} is out of range"))
}

/// `Action.from_json(raw)` (`engine/state.py:26-28`).
fn action_of(raw: &JsonValue) -> Result<Action, String> {
    let bad = || format!("not an action: {raw}");
    let items = raw.as_array().ok_or_else(bad)?;
    let kind = items
        .first()
        .and_then(JsonValue::as_str)
        .and_then(ActionKind::from_value)
        .ok_or_else(bad)?;
    let arg = |i: usize| -> Result<i32, String> {
        items
            .get(i)
            .and_then(JsonValue::as_i64)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(bad)
    };
    Ok(Action::new(kind, arg(1)?, arg(2)?, arg(3)?))
}

/// `label(st, uid)` (`blind_transcripts.py:67-73`).
/// Only hand cards are labelled here, so the `PLAYER_TARGET` case never arises.
fn label(st: &GameState, uid: Uid) -> String {
    if usize::from(uid) >= st.cards().len() {
        return format!("#{uid}");
    }
    let cd = st.def_of(uid);
    format!("{} {}[{uid}]", cd.card_number, cd.name)
}

/// `unit(st, dv, u)` (`blind_transcripts.py:76-90`).
fn unit(st: &GameState, dv: &gcg_engine::Derived, u: Uid) -> String {
    let c = st.card(u);
    let cd = st.def_of(u);
    let amounts = keywords(dv, u);
    let kws: Vec<&str> = Kw::SORTED_BY_VALUE
        .iter()
        .filter(|&&k| amounts.get(k).is_some())
        .map(|k| k.as_str())
        .collect();
    let kws = kws.join(",");
    let pilot = match Uid::try_from(c.pair) {
        Ok(p) => format!(
            " +{}{}",
            st.def_of(p).name,
            if is_linked(dv, u) { "(linked)" } else { "" }
        ),
        Err(_) => String::new(),
    };
    let hp = hp_of(st, dv, u);
    format!(
        "[{u}] {} {}{pilot} Lv{} AP{} HP{}/{hp} {}{}{}",
        cd.card_number,
        cd.name,
        cd.level,
        ap_of(st, dv, u),
        hp - c.damage,
        if c.rested { "rested" } else { "active" },
        if c.entered_turn == st.turn() {
            " (deployed this turn)"
        } else {
            ""
        },
        if kws.is_empty() {
            String::new()
        } else {
            format!(" {{{kws}}}")
        }
    )
}

/// `board(st, decider)` (`blind_transcripts.py:93-110`).
fn board(st: &GameState, decider: u8) -> Result<Vec<String>, String> {
    let dv = derived(st).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for p in [decider, 1 - decider] {
        let res = st.zone(p, Zone::ResourceArea);
        let active = res.iter().filter(|&&u| !st.card(u).rested).count();
        let ex = res
            .iter()
            .filter(|&&u| st.def_of(u).card_type == CardType::ExResource)
            .count();
        out.push(format!(
            "  P{p} ({}): shields {}, resources {active}/{} active, EX {ex}, deck {}, trash {}",
            if p == decider { "decider" } else { "opponent" },
            st.zone(p, Zone::Shield).len(),
            res.len(),
            st.zone(p, Zone::Deck).len(),
            st.zone(p, Zone::Trash).len()
        ));
        for &u in st.zone(p, Zone::Base) {
            out.push(format!("    base: {}", unit(st, &dv, u)));
        }
        for &u in st.zone(p, Zone::Battle) {
            out.push(format!("    unit: {}", unit(st, &dv, u)));
        }
        let hidden = if p == decider {
            ""
        } else {
            " (hidden from the decider)"
        };
        let hand: Vec<String> = st
            .zone(p, Zone::Hand)
            .iter()
            .map(|&u| label(st, u))
            .collect();
        let hand = if hand.is_empty() {
            "-".to_owned()
        } else {
            hand.join(", ")
        };
        out.push(format!("    hand{hidden}: {hand}"));
    }
    Ok(out)
}

/// `Decision.prompt` as Python stores it (`engine/state.py:42`).
fn prompt_text(st: &GameState, p: Prompt) -> String {
    match p {
        Prompt::Engine(e) => e.as_str().to_owned(),
        Prompt::Ask(id) => st.registry().str(id).to_owned(),
        Prompt::Choose(v) => format!("choose {}", st.registry().var_name(v)),
    }
}

/// The options line: `sorted({describe_action(st, dec, o) for o in dec.options})`.
fn options_line(st: &GameState, dec: &DecisionView<'_>) -> String {
    let options: BTreeSet<String> = dec
        .options
        .iter()
        .map(|&o| describe_action(st, dec, o))
        .collect();
    let options: Vec<String> = options.into_iter().collect();
    format!("  options: {}", options.join(" | "))
}

/// `transcript(doc)` (`blind_transcripts.py:113-136`): the lines of a replay that
/// `check_replay` accepted.
///
/// # Errors
/// A malformed field, or an engine error while re-playing.
pub fn transcript(doc: &JsonValue) -> Result<Vec<String>, String> {
    let seats = sorted_seats(doc)?;
    let (Some(first), Some(second)) = (seats.first(), seats.get(1)) else {
        return Err("the replay has fewer than two seats".to_owned());
    };
    let decks = [decklist(first)?, decklist(second)?];
    let refs: Vec<Vec<&str>> = decks
        .iter()
        .flat_map(|d| {
            d.iter()
                .map(|side| side.iter().map(String::as_str).collect())
        })
        .collect();
    let chooser = match field(doc, "chooser")? {
        JsonValue::Null => None,
        c => Some(
            c.as_u64()
                .and_then(|c| u8::try_from(c).ok())
                .ok_or("the replay's chooser is not a seat")?,
        ),
    };
    let seed = field(doc, "seed")?
        .as_u64()
        .ok_or("the replay's seed is out of range")?;
    let opts = NewGameOptions {
        chooser,
        turn_limit: int_field(doc, "turn_limit")?,
        max_actions: int_field(doc, "max_actions")?,
        validate: true,
    };
    let mut st = new_game_by_number(
        &Registry::embedded(),
        [(&refs[0], &refs[1]), (&refs[2], &refs[3])],
        seed,
        opts,
    )
    .map_err(|e| e.to_string())?;
    let actions = field(doc, "actions")?
        .as_array()
        .ok_or("the replay's actions are not a list")?;
    let mut lines: Vec<String> = Vec::new();
    for (i, raw) in actions.iter().enumerate() {
        let action = action_of(raw)?;
        let (head, played) = {
            let Some(dec) = st.pending() else {
                return Err(format!("action #{i}: no decision is pending"));
            };
            let head = format!(
                "#{i} turn {} P{} {}",
                st.turn(),
                dec.hdr.player,
                dec.hdr.kind.as_str()
            );
            let played = describe_action(&st, &dec, action);
            if dec.options.len() > 1 {
                let prompt = prompt_text(&st, dec.hdr.prompt);
                if prompt.is_empty() {
                    lines.push(format!("\n{head}"));
                } else {
                    lines.push(format!("\n{head} prompt: {prompt}"));
                }
                lines.extend(board(&st, dec.hdr.player)?);
                lines.push(options_line(&st, &dec));
                lines.push(format!("  PLAYED: {played}"));
                (None, played)
            } else {
                (Some(head), played)
            }
        };
        if let Some(head) = head {
            lines.push(format!("{head}: {played} (only option)"));
        }
        apply(&mut st, action, true).map_err(|e| format!("action #{i}: {e}"))?;
    }
    let winner = st
        .winner()
        .map_or_else(|| "None".to_owned(), |w| w.to_string());
    let reason = st.end_reason().map_or("None", |r| r.as_str());
    lines.push(format!(
        "\nGAME OVER: winner P{winner} on turn {} ({reason})",
        st.turn()
    ));
    Ok(lines)
}

/// The file text of a transcript: its lines joined, with a final newline.
///
/// # Errors
/// What [`transcript`] returns.
pub fn transcript_text(doc: &JsonValue) -> Result<String, String> {
    Ok(transcript(doc)?.join("\n") + "\n")
}

/// The files `run.rglob("replays/*.json")` yields: `*.json` entries directly inside a
/// directory named `replays` at any depth below `run` (directory symlinks are not followed),
/// sorted as `sorted()` sorts paths, by their parts.
fn replay_files(run: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![run.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let in_replays = dir != run && dir.file_name().is_some_and(|n| n == "replays");
        for e in entries.filter_map(Result::ok) {
            let path = e.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path.clone());
            }
            let is_json = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".json"));
            if in_replays && is_json && path.is_file() {
                found.push(path);
            }
        }
    }
    let parts = |p: &PathBuf| -> Vec<String> {
        py_path_str(p)
            .split('/')
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    found.sort_by_key(parts);
    if found.is_empty() && !run.is_dir() {
        return Err(format!("{}: not a directory", py_path_str(run)));
    }
    Ok(found)
}

/// `str(list_of_str)`: Python's list repr.
fn py_list(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| py_repr(s)).collect();
    format!("[{}]", inner.join(", "))
}

/// The key of a transcripts run: each id with every replay file of the game, in order.
pub type TranscriptKey = Vec<(String, Vec<String>)>;

/// `write_transcripts(out, key_file, runs)` (`blind_transcripts.py:139-155`).
///
/// # Errors
/// A replay that does not reproduce its outcome, an unreadable or malformed file, or a write
/// failure.
pub fn write_transcripts(
    out: &Path,
    key_file: &Path,
    runs: &[PathBuf],
) -> Result<TranscriptKey, String> {
    let shown = |p: &Path| py_path_str(p);
    std::fs::create_dir_all(out).map_err(|e| format!("cannot create {}: {e}", shown(out)))?;
    let mut key: TranscriptKey = Vec::new();
    for run in runs {
        for path in replay_files(run)? {
            let name = shown(&path);
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("cannot read {name}: {e}"))?;
            let doc: JsonValue =
                serde_json::from_str(&text).map_err(|e| format!("{name}: not JSON: {e}"))?;
            let check = check_replay(&doc).map_err(|e| format!("{name}: {e}"))?;
            if !check.ok {
                return Err(format!(
                    "{name}: the replay does not reproduce: {}",
                    py_list(&check.mismatches)
                ));
            }
            let gid = game_id(&doc).map_err(|e| format!("{name}: {e}"))?;
            match key.iter_mut().find(|(id, _)| *id == gid) {
                Some((_, files)) => files.push(name),
                None => {
                    let body = transcript_text(&doc).map_err(|e| format!("{name}: {e}"))?;
                    let file = out.join(format!("game-{gid}.txt"));
                    std::fs::write(&file, body)
                        .map_err(|e| format!("cannot write {}: {e}", shown(&file)))?;
                    key.push((gid, vec![name]));
                }
            }
        }
    }
    let doc = Json::Object(
        key.iter()
            .map(|(id, files)| {
                (
                    Key::Str(id.clone()),
                    Json::Array(files.iter().map(|f| Json::from(f.as_str())).collect()),
                )
            })
            .collect(),
    );
    let options = DumpOptions {
        indent: Some(1),
        sort_keys: true,
        compact: false,
        ensure_ascii: true,
    };
    std::fs::write(key_file, dumps(&doc, options) + "\n")
        .map_err(|e| format!("cannot write {}: {e}", shown(key_file)))?;
    Ok(key)
}

/// `gcg-eval transcripts`: writes the transcripts and the key, then prints
/// `{n} transcripts in {out}; key in {key_file}`.
///
/// # Errors
/// What [`write_transcripts`] returns.
pub fn run(args: &TranscriptsArgs, out: &mut dyn Write) -> Result<(), CommandError> {
    let key =
        write_transcripts(&args.out, &args.key_file, &args.runs).map_err(CommandError::Invalid)?;
    let line = PyStr::from(
        format!(
            "{} transcripts in {}; key in {}",
            key.len(),
            py_path_str(&args.out),
            py_path_str(&args.key_file)
        )
        .as_str(),
    );
    let _ = out.write_all(&line.to_surrogateescape_bytes());
    let _ = out.write_all(b"\n");
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn doc() -> JsonValue {
        json!({
            "format": "gcg-sim-replay",
            "version": 1,
            "seats": [
                {"seat": 1, "agent": "b", "agent_seed": 2, "main": {"GD01-030": 4}, "resources": {"R-001": 10}},
                {"seat": 0, "agent": "a", "agent_seed": 1, "main": {"GD01-001": 4}, "resources": {"R-001": 10}},
            ],
            "seed": 3,
            "chooser": null,
            "turn_limit": 200,
            "max_actions": 20000,
            "actions": [["go_first", 0, -1, -1]],
            "decision_log": [{"visits": 9}],
        })
    }

    #[test]
    fn the_id_is_the_games_alone() {
        let base = game_id(&doc()).expect("an id");
        assert_eq!(base.len(), 12);
        let mut other = doc();
        for s in other["seats"].as_array_mut().expect("seats") {
            s["agent"] = json!("other");
            s["agent_seed"] = json!(0);
        }
        other["decision_log"] = json!([]);
        assert_eq!(
            game_id(&other).expect("an id"),
            base,
            "agents and logs stay out"
        );
        let mut swapped = doc();
        swapped["seats"].as_array_mut().expect("seats").swap(0, 1);
        assert_eq!(
            game_id(&swapped).expect("an id"),
            base,
            "seats sort by number"
        );
        let mut fewer = doc();
        fewer["actions"] = json!([]);
        assert_ne!(game_id(&fewer).expect("an id"), base, "the actions count");
    }

    #[test]
    fn the_id_hashes_pythons_canonical_json() {
        // json.dumps({"seed": 3, "chooser": None, "turn_limit": 200, "max_actions": 20000,
        // "actions": [["go_first", 0, -1, -1]], "decks": [{"main": {"GD01-001": 4},
        // "resources": {"R-001": 10}}, {"main": {"GD01-030": 4}, "resources": {"R-001": 10}}]},
        // sort_keys=True, separators=(",", ":"), ensure_ascii=True)
        let text = r#"{"actions":[["go_first",0,-1,-1]],"chooser":null,"decks":[{"main":{"GD01-001":4},"resources":{"R-001":10}},{"main":{"GD01-030":4},"resources":{"R-001":10}}],"max_actions":20000,"seed":3,"turn_limit":200}"#;
        let hex: String = Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(game_id(&doc()).expect("an id"), hex[..12]);
    }

    #[test]
    fn python_list_repr() {
        assert_eq!(
            py_list(&["a: x".to_owned(), "it's".to_owned()]),
            "['a: x', \"it's\"]"
        );
        assert_eq!(py_list(&[]), "[]");
    }
}
