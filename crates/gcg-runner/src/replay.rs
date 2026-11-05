//! Self-contained replay files: re-simulating a game for its readable log, building the replay
//! document, and checking a document against its recorded outcome (`runner/replayfile.py`;
//! docs/port/inventory/01-rng-runner-cli.md §2.10).
//!
//! `check_replay` reads a `json.loads` document, so it reproduces the Python operations it
//! applies to the values (`int()`, `d[key]`, `list()`, `sorted(key=...)`, `==`, `repr`) and the
//! order in which they run: a malformed document fails with the exception Python raises first
//! (Inv 01 hazards 34-39). Where Python's text is CPython's own exception text, the class and
//! the message are CPython's for the cases below; values outside the Rust types (a seed outside
//! `[0, 2**64)`, a chooser that is not a seat, an action argument outside `i32`) are reported
//! as `ValueError`s that Python would not raise (it would replay them, PLAN A19).

use std::fmt::Write as _;

use gcg_core::pyfloat::repr_f64;
use gcg_core::pyjson::Json;
use gcg_data::{MAX_EXPANDED_CARDS, py_repr};
use gcg_effects::Registry;
use gcg_engine::{
    Action, ActionKind, ApplyError, DecisionKind, DecisionView, EngineError, GameState,
    NewGameError, NewGameOptions, PLAYER_TARGET, apply, canon, new_game, new_game_by_number,
};
use serde_json::Value;

use crate::argparse::{PyStr, pyint};
use crate::config::seat_decks;
use crate::reports::{Resimulation, replay_document_with};
use crate::{BenchmarkConfig, GameRecord, REPLAY_FORMAT, REPLAY_VERSION, ReplayCheck, ReplayError};

/// `CARD_CHOICES` (`runner/replayfile.py:23`).
const CARD_CHOICES: [DecisionKind; 3] = [
    DecisionKind::Select,
    DecisionKind::Discard,
    DecisionKind::Excess,
];

/// `_label(st, uid)` (`runner/replayfile.py:30-34`): `"{card_number} {name}"`, or the bare
/// number when it is not a card index.
fn label(st: &GameState, uid: i32) -> String {
    match u16::try_from(uid) {
        Ok(u) if usize::from(u) < st.cards().len() => {
            let cdef = st.def_of(u);
            format!("{} {}", cdef.card_number, cdef.name)
        }
        _ => uid.to_string(),
    }
}

/// `_describe_choice(st, dec, a)` (`runner/replayfile.py:37-42`); `'bottom' if a.a else
/// 'top'` is Python truthiness (Inv 01 hazard 32).
fn describe_choice(st: &GameState, dec: &DecisionView<'_>, a: Action) -> String {
    let kind = dec.hdr.kind;
    if kind == DecisionKind::Arrange && a.b != gcg_engine::NO_ARG {
        let side = if a.a != 0 { "bottom" } else { "top" };
        return format!("{side}: {}", label(st, a.b));
    }
    if CARD_CHOICES.contains(&kind)
        && dec.hdr.ctx(gcg_engine::CtxKey::Mode) == gcg_engine::NO_ARG
        && dec.hdr.ctx(gcg_engine::CtxKey::RestSub) == gcg_engine::NO_ARG
    {
        return format!("select {}", label(st, a.a));
    }
    format!("select option {}", a.a)
}

/// `describe_action(st, dec, a)` (`runner/replayfile.py:45-65`): a readable description of
/// `a` in state `st`, before it is applied, ending with `[{a}]` (`Action.__str__`).
#[must_use]
pub fn describe_action(st: &GameState, dec: &DecisionView<'_>, a: Action) -> String {
    let text = match a.kind {
        ActionKind::PlayUnit | ActionKind::PlayBase | ActionKind::PlayCommand => {
            format!("play {}", label(st, a.a))
        }
        ActionKind::Pair => format!("pair {} with {}", label(st, a.a), label(st, a.b)),
        ActionKind::Activate => format!("activate {}", label(st, a.a)),
        ActionKind::Attack => {
            let target = if a.b == PLAYER_TARGET {
                "the player".to_owned()
            } else {
                label(st, a.b)
            };
            format!("attack {target} with {}", label(st, a.a))
        }
        ActionKind::Block => format!("block with {}", label(st, a.a)),
        ActionKind::GoFirst => format!("P{} goes first", a.a),
        ActionKind::Select => describe_choice(st, dec, a),
        other => other.as_str().to_owned(),
    };
    format!("{text} [{a}]")
}

/// An exception of one of the classes `check_replay` turns into `"malformed replay document:
/// {Class}: {exc}"` (`runner/replayfile.py:180-183`).
fn malformed(py_class: &'static str, detail: impl Into<String>) -> ReplayError {
    ReplayError::Malformed {
        py_class,
        detail: detail.into(),
    }
}

/// An engine error raised while re-simulating: `check_replay` catches the `KeyError`,
/// `TypeError`, `IndexError` and `ValueError` classes (`runner/replayfile.py:182`); any
/// other class escapes (a traceback in Python).
fn engine_replay_error(e: EngineError) -> ReplayError {
    match e.py_class() {
        class @ ("KeyError" | "TypeError" | "IndexError" | "ValueError") => {
            malformed(class, e.to_string())
        }
        _ => ReplayError::Engine(e),
    }
}

/// What `new_game` raises while re-simulating (`runner/replayfile.py:84-87`): an
/// unimplemented card is the replay's own error; a `DeckConstructionError` is a `ValueError`.
pub(crate) fn new_game_replay_error(e: NewGameError) -> ReplayError {
    match e {
        NewGameError::Unimplemented(u) => ReplayError::Unimplemented(u),
        NewGameError::Construction(c) => malformed("DeckConstructionError", c.to_string()),
        NewGameError::Engine(e) => engine_replay_error(e),
    }
}

/// The loop of `replay_with_log` (`runner/replayfile.py:88-101`) after `new_game`:
/// `next_action(index)` reads the next recorded action (`Action.from_json(list(raw))`), which
/// runs before the "more actions" check (Inv 01 hazard 35).
fn replay_actions(
    st: &mut GameState,
    count: usize,
    next_action: &mut dyn FnMut(usize) -> Result<Action, ReplayError>,
) -> Result<Vec<String>, ReplayError> {
    let mut log = Vec::with_capacity(count + 1);
    for index in 0..count {
        let action = next_action(index)?;
        let Some(dec) = st.pending() else {
            return Err(ReplayError::ExtraActions);
        };
        log.push(format!(
            "T{} P{} {}: {}",
            st.turn(),
            dec.hdr.player,
            dec.hdr.kind.as_str(),
            describe_action(st, &dec, action)
        ));
        apply(st, action, true).map_err(|e| match e {
            ApplyError::Illegal(_) => ReplayError::Illegal { index, action },
            ApplyError::Engine(e) => engine_replay_error(e),
        })?;
    }
    let reason = st.end_reason().map_or("not finished", |r| r.as_str());
    let winner = st
        .winner()
        .map_or_else(|| "None".to_owned(), |w| w.to_string());
    log.push(format!(
        "T{} game over: winner P{winner} ({reason})",
        st.turn()
    ));
    Ok(log)
}

/// `replay_with_log(seat_decks(config, m), record.seed, record.chooser, record.actions, ...)`
/// (`runner/replayfile.py:75-101`, as `replay_document` calls it at `:105-113`): the
/// re-simulated winner seat and the readable log of a recorded game.
///
/// # Errors
/// What re-simulating raises, as a [`ReplayError`].
pub fn resimulate(record: &GameRecord, cfg: &BenchmarkConfig) -> Result<Resimulation, ReplayError> {
    let decks =
        seat_decks(cfg, record.match_index).map_err(|e| malformed("DeckError", e.to_string()))?;
    let opts = NewGameOptions {
        chooser: record.chooser,
        turn_limit: record.turn_limit,
        max_actions: record.max_actions,
        validate: true,
    };
    let mut st = new_game(
        &Registry::embedded(),
        [&decks[0], &decks[1]],
        record.seed,
        opts,
    )
    .map_err(new_game_replay_error)?;
    let log = replay_actions(
        &mut st,
        record.actions.len(),
        &mut |i| Ok(record.actions[i]),
    )?;
    Ok(Resimulation {
        winner_seat: st.winner(),
        log,
    })
}

/// `replay_document(record, config, label)` (`runner/replayfile.py:104-150`).
///
/// # Errors
/// What re-simulating the game raises.
pub fn replay_document(
    record: &GameRecord,
    cfg: &BenchmarkConfig,
    label: &str,
) -> Result<Json, ReplayError> {
    let sim = resimulate(record, cfg)?;
    Ok(replay_document_with(record, cfg, label, &sim))
}

// ---------------------------------------------------------------------------------------------
// Python operations on json.loads values

/// `type(v).__name__`.
fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// `repr(v)` of a `json.loads` value.
fn repr_value(v: &Value) -> String {
    match v {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(n) => n.as_i64().map_or_else(
            || {
                n.as_u64().map_or_else(
                    || repr_f64(n.as_f64().unwrap_or(f64::NAN)),
                    |u| u.to_string(),
                )
            },
            |i| i.to_string(),
        ),
        Value::String(s) => py_repr(s),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(repr_value).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let mut out = String::from("{");
            for (i, (k, x)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{}: {}", py_repr(k), repr_value(x));
            }
            out.push('}');
            out
        }
    }
}

/// `str(v)` of a `json.loads` value (`cli.py:359-360` prints the seats with it).
pub(crate) fn py_str_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => repr_value(other),
    }
}

/// `int(s["seat"])`, the sort key of the seats (`runner/replayfile.py:169`, `cli.py:359`).
///
/// # Errors
/// What `s["seat"]` or `int()` raises.
pub(crate) fn seat_key(seat: &Value) -> Result<i128, ReplayError> {
    py_int(getitem(seat, "seat")?)
}

/// `v[key]` for a `str` key.
pub(crate) fn getitem<'v>(v: &'v Value, key: &str) -> Result<&'v Value, ReplayError> {
    match v {
        Value::Object(map) => map
            .get(key)
            .ok_or_else(|| malformed("KeyError", py_repr(key))),
        Value::Array(_) => Err(malformed(
            "TypeError",
            "list indices must be integers or slices, not str",
        )),
        Value::String(_) => Err(malformed(
            "TypeError",
            "string indices must be integers, not 'str'",
        )),
        other => Err(malformed(
            "TypeError",
            format!("'{}' object is not subscriptable", type_name(other)),
        )),
    }
}

/// `list(v)`: a list's items, a dict's keys, a string's characters.
fn to_list(v: &Value) -> Result<Vec<Value>, ReplayError> {
    match v {
        Value::Array(items) => Ok(items.clone()),
        Value::Object(map) => Ok(map.keys().map(|k| Value::String(k.clone())).collect()),
        Value::String(s) => Ok(s.chars().map(|c| Value::String(c.to_string())).collect()),
        other => Err(malformed(
            "TypeError",
            format!("'{}' object is not iterable", type_name(other)),
        )),
    }
}

/// `items[i]` on a list (`IndexError` past the end).
fn index(items: &[Value], i: usize) -> Result<&Value, ReplayError> {
    items
        .get(i)
        .ok_or_else(|| malformed("IndexError", "list index out of range"))
}

/// `int(v)` of a `json.loads` value: booleans are 0 and 1, floats truncate toward zero,
/// strings parse as `int(text)` (base 10, surrounding whitespace, underscores, Unicode
/// digits).
fn py_int(v: &Value) -> Result<i128, ReplayError> {
    match v {
        Value::Bool(b) => Ok(i128::from(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                return Ok(i128::from(i));
            }
            if let Some(u) = n.as_u64() {
                return Ok(i128::from(u));
            }
            let x = n.as_f64().unwrap_or(f64::NAN);
            if x.is_nan() {
                return Err(malformed(
                    "ValueError",
                    "cannot convert float NaN to integer",
                ));
            }
            if x.is_infinite() {
                return Err(malformed(
                    "OverflowError",
                    "cannot convert float infinity to integer",
                ));
            }
            let t = x.trunc();
            if t.abs() < 1.7e38 {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "|t| < 2^127 and integral, so the conversion is exact"
                )]
                Ok(t as i128)
            } else {
                Err(out_of_range("integer", &repr_f64(t)))
            }
        }
        Value::String(s) => pyint::parse_int(&PyStr::from(s.as_str()), 10)
            .and_then(|i| i.to_decimal().parse::<i128>().ok())
            .ok_or_else(|| {
                malformed(
                    "ValueError",
                    format!("invalid literal for int() with base 10: {}", py_repr(s)),
                )
            }),
        other => Err(malformed(
            "TypeError",
            format!(
                "int() argument must be a string, a bytes-like object or a real number, not '{}'",
                type_name(other)
            ),
        )),
    }
}

/// A value Python accepts but the Rust types cannot hold (module docs).
fn out_of_range(what: &str, value: &str) -> ReplayError {
    malformed(
        "ValueError",
        format!("{what} {value} is outside the range this simulator supports"),
    )
}

fn int_in<T: TryFrom<i128>>(v: &Value, what: &str) -> Result<T, ReplayError> {
    let i = py_int(v)?;
    T::try_from(i).map_err(|_| out_of_range(what, &i.to_string()))
}

/// `_decklist(seat)` (`runner/replayfile.py:68-72`): each side's `{card: count}` expanded
/// (`range(int(c))`, so a negative count gives no copies) and sorted.
fn decklist(seat: &Value) -> Result<[Vec<String>; 2], ReplayError> {
    let mut out: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    for (side, key) in out.iter_mut().zip(["main", "resources"]) {
        let counts = getitem(seat, key)?;
        let Value::Object(map) = counts else {
            return Err(malformed(
                "AttributeError",
                format!("'{}' object has no attribute 'items'", type_name(counts)),
            ));
        };
        let mut total: u128 = 0;
        for (number, count) in map {
            let n = py_int(count)?;
            let copies = u128::try_from(n.max(0)).unwrap_or(0);
            total = total.saturating_add(copies);
            if total > MAX_EXPANDED_CARDS {
                return Err(out_of_range("deck size", &total.to_string()));
            }
            let copies = usize::try_from(copies).unwrap_or(usize::MAX);
            side.extend(std::iter::repeat_n(number.clone(), copies));
        }
        side.sort();
    }
    Ok(out)
}

/// `Action.from_json(list(raw))` (`engine/state.py:26-28`, `runner/replayfile.py:90`).
fn action_from_raw(raw: &Value) -> Result<Action, ReplayError> {
    let data = to_list(raw)?;
    let first = index(&data, 0)?;
    let kind = first
        .as_str()
        .and_then(ActionKind::from_value)
        .ok_or_else(|| {
            malformed(
                "ValueError",
                format!("{} is not a valid ActionKind", repr_value(first)),
            )
        })?;
    let arg =
        |i: usize| -> Result<i32, ReplayError> { int_in(index(&data, i)?, "action argument") };
    Ok(Action::new(kind, arg(1)?, arg(2)?, arg(3)?))
}

/// `expected.get(k) != actual` with Python's `==` between a `json.loads` value and an `int`.
fn int_eq(v: Option<&Value>, x: i64) -> bool {
    match v {
        Some(Value::Bool(b)) => i64::from(*b) == x,
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                i == x
            } else if n.as_u64().is_some() {
                false
            } else {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "an int equals a float only when the float is exactly that int; \
                              |x| < 2^31 here, so the conversion is exact"
                )]
                let xf = x as f64;
                n.as_f64() == Some(xf)
            }
        }
        _ => false,
    }
}

/// One `actual` value of `check_replay` (`runner/replayfile.py:186-191`).
enum Actual<'a> {
    Int(Option<i64>),
    Str(Option<&'a str>),
}

impl Actual<'_> {
    fn equals(&self, v: Option<&Value>) -> bool {
        match self {
            Self::Int(None) | Self::Str(None) => matches!(v, None | Some(Value::Null)),
            Self::Int(Some(x)) => int_eq(v, *x),
            Self::Str(Some(s)) => v.and_then(Value::as_str) == Some(*s),
        }
    }

    fn repr(&self) -> String {
        match self {
            Self::Int(None) | Self::Str(None) => "None".to_owned(),
            Self::Int(Some(x)) => x.to_string(),
            Self::Str(Some(s)) => py_repr(s),
        }
    }
}

/// `check_replay(doc)` (`runner/replayfile.py:164-205`): re-simulate a replay document and
/// compare with its expected outcome. Each seat's deck is the sorted expansion of the written
/// numbers, played through `new_game_by_number`, so an unknown or wrong number fails as
/// Python's `DeckConstructionError`.
///
/// # Errors
/// [`ReplayError::NotReplay`] for a document of another format or version; the replay's own
/// errors; [`ReplayError::Malformed`] with the class and message of the first Python exception
/// a malformed document raises.
pub fn check_replay(doc: &Value) -> Result<ReplayCheck, ReplayError> {
    let Value::Object(map) = doc else {
        return Err(malformed(
            "AttributeError",
            format!("'{}' object has no attribute 'get'", type_name(doc)),
        ));
    };
    let format_ok = map.get("format").and_then(Value::as_str) == Some(REPLAY_FORMAT);
    if !format_ok || !int_eq(map.get("version"), i64::from(REPLAY_VERSION)) {
        return Err(ReplayError::NotReplay);
    }
    // sorted(doc["seats"], key=lambda s: int(s["seat"])): every key first, then a stable sort.
    let seats = to_list(getitem(doc, "seats")?)?;
    let keys = seats
        .iter()
        .map(seat_key)
        .collect::<Result<Vec<i128>, ReplayError>>()?;
    let mut order: Vec<usize> = (0..seats.len()).collect();
    order.sort_by_key(|&i| keys[i]);
    let seat = |k: usize| index(&seats, order.get(k).copied().unwrap_or(usize::MAX));
    let first = decklist(seat(0)?)?;
    let decks = [first, decklist(seat(1)?)?];
    // replay_with_log(decks, int(doc["seed"]), chooser, doc["actions"], turn_limit=..., ...):
    // the arguments are evaluated left to right (Inv 01 hazard 38). The range checks of the
    // Rust types come after every Python operation.
    let seed = py_int(getitem(doc, "seed")?)?;
    let chooser = match getitem(doc, "chooser")? {
        Value::Null => None,
        c => Some(py_int(c)?),
    };
    let actions = getitem(doc, "actions")?;
    let turn_limit = py_int(getitem(doc, "turn_limit")?)?;
    let max_actions = py_int(getitem(doc, "max_actions")?)?;
    let seed = u64::try_from(seed).map_err(|_| out_of_range("seed", &seed.to_string()))?;
    let chooser = chooser
        .map(|c| {
            u8::try_from(c)
                .ok()
                .filter(|&c| c < 2)
                .ok_or_else(|| out_of_range("chooser", &c.to_string()))
        })
        .transpose()?;
    let turn_limit = i32::try_from(turn_limit)
        .map_err(|_| out_of_range("turn_limit", &turn_limit.to_string()))?;
    let max_actions = i64::try_from(max_actions)
        .map_err(|_| out_of_range("max_actions", &max_actions.to_string()))?;
    let refs: Vec<Vec<&str>> = decks
        .iter()
        .flat_map(|d| {
            d.iter()
                .map(|side| side.iter().map(String::as_str).collect())
        })
        .collect();
    let opts = NewGameOptions {
        chooser,
        turn_limit,
        max_actions,
        validate: true,
    };
    let mut st = new_game_by_number(
        &Registry::embedded(),
        [(&refs[0], &refs[1]), (&refs[2], &refs[3])],
        seed,
        opts,
    )
    .map_err(new_game_replay_error)?;
    let raw_actions = to_list(actions)?;
    let log = replay_actions(&mut st, raw_actions.len(), &mut |i| {
        action_from_raw(&raw_actions[i])
    })?;
    let expected = getitem(doc, "expected")?;
    let Value::Object(expected) = expected else {
        return Err(malformed(
            "AttributeError",
            format!("'{}' object has no attribute 'get'", type_name(expected)),
        ));
    };
    let end_reason = st.end_reason();
    let digest = canon::state_digest(&st);
    let actual = [
        ("winner_seat", Actual::Int(st.winner().map(i64::from))),
        ("end_reason", Actual::Str(end_reason.map(|r| r.as_str()))),
        ("turns", Actual::Int(Some(i64::from(st.turn())))),
        ("final_state_sha256", Actual::Str(Some(&digest))),
    ];
    let mismatches: Vec<String> = actual
        .iter()
        .filter(|(k, v)| !v.equals(expected.get(*k)))
        .map(|(k, v)| {
            let shown = expected
                .get(*k)
                .map_or_else(|| "None".to_owned(), repr_value);
            format!("{k}: expected {shown}, replay gave {}", v.repr())
        })
        .collect();
    Ok(ReplayCheck {
        ok: mismatches.is_empty(),
        mismatches,
        winner_seat: st.winner(),
        end_reason,
        turns: st.turn(),
        final_state_sha256: digest,
        log,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn err(doc: &Value) -> String {
        check_replay(doc).expect_err("malformed").to_string()
    }

    fn doc() -> Value {
        json!({
            "format": "gcg-sim-replay",
            "version": 1,
            "seats": [
                {"seat": 1, "main": {"GD01-030": 4}, "resources": {"R-001": 10}},
                {"seat": 0, "main": {"GD01-001": 4}, "resources": {"R-001": 10}},
            ],
            "seed": 3,
            "chooser": null,
            "turn_limit": 200,
            "max_actions": 20000,
            "actions": [],
            "expected": {},
        })
    }

    #[test]
    fn other_formats_and_versions_are_not_replays() {
        // runner/replayfile.py:166-167; True == 1 == 1.0 in Python (Inv 01 hazard 36).
        let mut d = doc();
        d["format"] = json!("other");
        assert_eq!(err(&d), "not a gcg-sim-replay v1 document");
        for version in [json!(2), json!("1"), json!(null), json!(1.5)] {
            let mut d = doc();
            d["version"] = version;
            assert_eq!(err(&d), "not a gcg-sim-replay v1 document");
        }
        let mut d = doc();
        d.as_object_mut().expect("object").remove("version");
        assert_eq!(err(&d), "not a gcg-sim-replay v1 document");
        for version in [json!(1.0), json!(true)] {
            let mut d = doc();
            d["version"] = version;
            d["seats"] = json!(5);
            assert_eq!(
                err(&d),
                "malformed replay document: TypeError: 'int' object is not iterable"
            );
        }
    }

    #[test]
    fn malformed_documents_fail_with_pythons_first_exception() {
        type Mutate = fn(&mut Value);
        let cases: Vec<(Mutate, &str)> = vec![
            (
                |d| {
                    d.as_object_mut().expect("o").remove("seats");
                },
                "KeyError: 'seats'",
            ),
            (
                |d| d["seats"] = json!("ab"),
                "TypeError: string indices must be integers, not 'str'",
            ),
            (
                |d| d["seats"] = json!([[1]]),
                "TypeError: list indices must be integers or slices, not str",
            ),
            (
                |d| d["seats"] = json!([3]),
                "TypeError: 'int' object is not subscriptable",
            ),
            (|d| d["seats"] = json!([{}]), "KeyError: 'seat'"),
            (
                |d| d["seats"] = json!([{"seat": "x"}]),
                "ValueError: invalid literal for int() with base 10: 'x'",
            ),
            (
                |d| d["seats"] = json!([{"seat": null}]),
                "TypeError: int() argument must be a string, a bytes-like object or a real number, not 'NoneType'",
            ),
            (
                |d| d["seats"] = json!([{"seat": 0, "main": {}, "resources": {}}]),
                "IndexError: list index out of range",
            ),
            (
                |d| d["seats"][1]["main"] = json!({"GD01-001": "four"}),
                "ValueError: invalid literal for int() with base 10: 'four'",
            ),
            (
                |d| d["seats"][1]["resources"] = json!([1]),
                "AttributeError: 'list' object has no attribute 'items'",
            ),
            (
                |d| {
                    d.as_object_mut().expect("o").remove("seed");
                },
                "KeyError: 'seed'",
            ),
            (
                |d| d["seed"] = json!([1]),
                "TypeError: int() argument must be a string, a bytes-like object or a real number, not 'list'",
            ),
            (
                |d| d["seed"] = json!(-1),
                "ValueError: seed -1 is outside the range this simulator supports",
            ),
            (
                |d| d["chooser"] = json!(2),
                "ValueError: chooser 2 is outside the range this simulator supports",
            ),
            (
                |d| {
                    d.as_object_mut().expect("o").remove("chooser");
                },
                "KeyError: 'chooser'",
            ),
            (
                |d| {
                    d.as_object_mut().expect("o").remove("actions");
                },
                "KeyError: 'actions'",
            ),
            (
                |d| d["turn_limit"] = json!(2.0e10),
                "ValueError: turn_limit 20000000000 is outside the range this simulator supports",
            ),
            (
                |d| d["max_actions"] = json!(f64::MAX),
                "ValueError: integer 1.7976931348623157e+308 is outside the range this simulator supports",
            ),
        ];
        for (mutate, message) in cases {
            let mut d = doc();
            mutate(&mut d);
            assert_eq!(
                err(&d),
                format!("malformed replay document: {message}"),
                "{message}"
            );
        }
    }

    #[test]
    fn int_follows_python() {
        assert_eq!(py_int(&json!(true)).ok(), Some(1));
        assert_eq!(py_int(&json!(-2.7)).ok(), Some(-2));
        assert_eq!(py_int(&json!(" 1_0 ")).ok(), Some(10));
        assert_eq!(py_int(&json!("١٢")).ok(), Some(12));
        assert_eq!(py_int(&json!(u64::MAX)).ok(), Some(i128::from(u64::MAX)));
        assert!(py_int(&json!("1.5")).is_err());
        assert!(py_int(&json!({})).is_err());
    }

    #[test]
    fn actions_parse_as_action_from_json() {
        assert_eq!(
            action_from_raw(&json!(["attack", 5, -2, -1])).ok(),
            Some(Action::new(ActionKind::Attack, 5, -2, -1))
        );
        assert_eq!(
            action_from_raw(&json!(["end_main", "1", true, 2.9])).ok(),
            Some(Action::new(ActionKind::EndMain, 1, 1, 2))
        );
        let e = |raw: Value| action_from_raw(&raw).expect_err("malformed").to_string();
        assert_eq!(
            e(json!(["no-such-kind", 0, 0, 0])),
            "malformed replay document: ValueError: 'no-such-kind' is not a valid ActionKind"
        );
        assert_eq!(
            e(json!([5, 0, 0, 0])),
            "malformed replay document: ValueError: 5 is not a valid ActionKind"
        );
        assert_eq!(
            e(json!(["pass", 0])),
            "malformed replay document: IndexError: list index out of range"
        );
        assert_eq!(
            e(json!([])),
            "malformed replay document: IndexError: list index out of range"
        );
        assert_eq!(
            e(json!(7)),
            "malformed replay document: TypeError: 'int' object is not iterable"
        );
        assert_eq!(
            e(json!("pass")),
            "malformed replay document: ValueError: 'p' is not a valid ActionKind"
        );
    }

    #[test]
    fn values_print_as_python_reprs() {
        assert_eq!(repr_value(&json!(null)), "None");
        assert_eq!(repr_value(&json!(23.0)), "23.0");
        assert_eq!(repr_value(&json!("battle_damage")), "'battle_damage'");
        assert_eq!(repr_value(&json!([1, "a", false])), "[1, 'a', False]");
        assert_eq!(repr_value(&json!({"k": "it's"})), "{'k': \"it's\"}");
    }

    #[test]
    fn expected_values_compare_with_pythons_equality() {
        assert!(Actual::Int(Some(23)).equals(Some(&json!(23))));
        assert!(Actual::Int(Some(23)).equals(Some(&json!(23.0))));
        assert!(Actual::Int(Some(1)).equals(Some(&json!(true))));
        assert!(!Actual::Int(Some(23)).equals(Some(&json!("23"))));
        assert!(!Actual::Int(Some(23)).equals(None));
        assert!(Actual::Int(None).equals(None));
        assert!(Actual::Int(None).equals(Some(&json!(null))));
        assert!(!Actual::Int(None).equals(Some(&json!(0))));
        assert!(Actual::Str(Some("x")).equals(Some(&json!("x"))));
        assert!(!Actual::Str(Some("x")).equals(Some(&json!(["x"]))));
        assert_eq!(Actual::Str(Some("deck_out")).repr(), "'deck_out'");
        assert_eq!(Actual::Int(None).repr(), "None");
    }
}
