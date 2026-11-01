//! JSON of the game, match and decision-log records (`runner/records.py`, `ai/base.py:79-100`,
//! `tests/runner/random_agent.py:30-40`).
//!
//! `to_json` writes Python's insertion order (every report writer sorts keys,
//! `reports/writer.py:34`, `:56`). `from_json` reads the records back from `games.ndjson` lines
//! and exported fixtures; it is strict where Python's `int()`/`bool()`/`str()` coercions are
//! lenient (a value of the wrong JSON type is a [`SchemaError`] naming its JSON pointer).

use gcg_ai::{LogAlternative, LogEntry};
use gcg_core::pyjson::Json;
use gcg_engine::{Action, ActionKind, DecisionKind, EndReason};
use serde_json::Value;

use crate::{DutCardEvents, GameRecord, MatchRecord, Outcome, PlayEvent, Role, SchemaError};

/// `Action.to_json()` (`engine/state.py:23-24`): `[kind.value, a, b, c]`.
pub(crate) fn action_json(a: &Action) -> Json {
    Json::Array(vec![
        a.kind.as_str().into(),
        a.a.into(),
        a.b.into(),
        a.c.into(),
    ])
}

/// `[list(a) for a in actions]` (`runner/records.py:124`, `runner/replayfile.py:140`).
pub(crate) fn actions_json(actions: &[Action]) -> Json {
    Json::Array(actions.iter().map(action_json).collect())
}

/// One decision-log dict: `log_entry` (`ai/base.py:85-100`) for the AI agents, the test
/// random agent's dict (`tests/runner/random_agent.py:30-40`) when the labels are `None`.
pub(crate) fn log_entry_json(e: &LogEntry) -> Json {
    let mut o = Json::object();
    o.set("turn", e.turn);
    o.set("player", e.player);
    o.set("decision", e.decision.as_str());
    o.set("chosen", action_json(&e.chosen));
    if let Some(label) = &e.label {
        o.set("label", &**label);
    }
    o.set(
        "alternatives",
        Json::Array(
            e.alternatives
                .iter()
                .map(|alt| {
                    let mut a = Json::object();
                    a.set("action", action_json(&alt.action));
                    if let Some(label) = &alt.label {
                        a.set("label", &**label);
                    }
                    a.set("value", alt.value);
                    a.set("visits", alt.visits);
                    a
                })
                .collect(),
        ),
    );
    o
}

/// `None if log is None else list(log)` (`runner/records.py:131`, `runner/replayfile.py:149`).
pub(crate) fn decision_log_json(log: Option<&[LogEntry]>) -> Json {
    log.map_or(Json::Null, |entries| {
        Json::Array(entries.iter().map(log_entry_json).collect())
    })
}

fn strs(items: &[Box<str>]) -> Json {
    Json::Array(items.iter().map(|s| Json::Str(s.to_string())).collect())
}

impl PlayEvent {
    /// `PlayEvent.to_json` (`runner/records.py:26-27`): `[card_number, turn, own_turn]`.
    #[must_use]
    pub fn to_json(&self) -> Json {
        Json::Array(vec![
            (&*self.card_number).into(),
            self.turn.into(),
            self.own_turn.into(),
        ])
    }
}

impl DutCardEvents {
    /// `DutCardEvents.to_json` (`runner/records.py:50-59`).
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("initial_hand", strs(&self.initial_hand));
        o.set("redrew", self.redrew);
        o.set("opening_hand", strs(&self.opening_hand));
        o.set("drawn", strs(&self.drawn));
        o.set(
            "played",
            Json::Array(self.played.iter().map(PlayEvent::to_json).collect()),
        );
        o.set("never_played", strs(&self.never_played));
        o.set("in_hand_at_end", strs(&self.in_hand_at_end));
        o
    }
}

impl GameRecord {
    /// `GameRecord.to_json` (`runner/records.py:107-132`).
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("match_index", self.match_index);
        o.set("game_index", self.game_index);
        o.set("seed", self.seed);
        o.set("agent_seeds", self.agent_seeds.to_vec());
        o.set("agents", strs(&self.agents));
        o.set("dut_seat", self.dut_seat);
        o.set("chooser", self.chooser);
        o.set("chooser_seat", self.chooser_seat);
        o.set("first_player", self.first_player);
        o.set("dut_on_play", self.dut_on_play);
        o.set("winner", self.winner.as_str());
        o.set("end_reason", self.end_reason.as_str());
        o.set("match_point", self.match_point.map(Role::as_str));
        o.set("turns", self.turns);
        o.set("active_at_end", self.active_at_end);
        o.set("actions", actions_json(&self.actions));
        o.set("redraws", self.redraws.to_vec());
        o.set("dut_cards", self.dut_cards.to_json());
        o.set("bench_seen", strs(&self.bench_seen));
        o.set("final_state_sha256", self.final_state_sha256.as_str());
        o.set("turn_limit", self.turn_limit);
        o.set("max_actions", self.max_actions);
        o.set(
            "decision_log",
            decision_log_json(self.decision_log.as_deref()),
        );
        o
    }

    /// `GameRecord.from_json` (`runner/records.py:134-162`): a missing `decision_log` key reads
    /// as `None`, as `d.get("decision_log")` does.
    ///
    /// # Errors
    /// [`SchemaError`] naming the first missing key or value of the wrong type.
    pub fn from_json(v: &Value) -> Result<Self, SchemaError> {
        let at = Path::root();
        let seeds = arr(field(v, "agent_seeds", &at)?, &at.key("agent_seeds"))?;
        let agents = arr(field(v, "agents", &at)?, &at.key("agents"))?;
        let redraws = arr(field(v, "redraws", &at)?, &at.key("redraws"))?;
        let pair = |items: &[Value], name: &str| -> Result<(), SchemaError> {
            if items.len() == 2 {
                Ok(())
            } else {
                Err(at.key(name).error("expected two items"))
            }
        };
        pair(seeds, "agent_seeds")?;
        pair(agents, "agents")?;
        pair(redraws, "redraws")?;
        let p = at.key("agent_seeds");
        let agent_seeds = [
            u64_of(&seeds[0], &p.index(0))?,
            u64_of(&seeds[1], &p.index(1))?,
        ];
        let p = at.key("agents");
        let agent_names = [
            str_of(&agents[0], &p.index(0))?,
            str_of(&agents[1], &p.index(1))?,
        ];
        let p = at.key("redraws");
        let redraw_flags = [
            bool_of(&redraws[0], &p.index(0))?,
            bool_of(&redraws[1], &p.index(1))?,
        ];
        let actions_at = at.key("actions");
        let actions = arr(field(v, "actions", &at)?, &actions_at)?
            .iter()
            .enumerate()
            .map(|(i, a)| action_from_json(a, &actions_at.index(i)))
            .collect::<Result<Vec<Action>, SchemaError>>()?;
        let log_at = at.key("decision_log");
        let decision_log = match v.get("decision_log") {
            None | Some(Value::Null) => None,
            Some(log) => Some(
                arr(log, &log_at)?
                    .iter()
                    .enumerate()
                    .map(|(i, e)| entry_from_json(e, &log_at.index(i)))
                    .collect::<Result<Vec<LogEntry>, SchemaError>>()?,
            ),
        };
        Ok(Self {
            match_index: u32_of(field(v, "match_index", &at)?, &at.key("match_index"))?,
            game_index: u32_of(field(v, "game_index", &at)?, &at.key("game_index"))?,
            seed: u64_of(field(v, "seed", &at)?, &at.key("seed"))?,
            agent_seeds,
            agents: agent_names,
            dut_seat: seat_of(field(v, "dut_seat", &at)?, &at.key("dut_seat"))?,
            chooser: match field(v, "chooser", &at)? {
                Value::Null => None,
                c => Some(seat_of(c, &at.key("chooser"))?),
            },
            chooser_seat: seat_of(field(v, "chooser_seat", &at)?, &at.key("chooser_seat"))?,
            first_player: seat_of(field(v, "first_player", &at)?, &at.key("first_player"))?,
            dut_on_play: bool_of(field(v, "dut_on_play", &at)?, &at.key("dut_on_play"))?,
            winner: outcome_of(field(v, "winner", &at)?, &at.key("winner"))?,
            end_reason: {
                let p = at.key("end_reason");
                let s = str_of(field(v, "end_reason", &at)?, &p)?;
                EndReason::from_value(&s).ok_or_else(|| p.error("not an EndReason value"))?
            },
            match_point: match field(v, "match_point", &at)? {
                Value::Null => None,
                m => Some(role_of(m, &at.key("match_point"))?),
            },
            turns: i32_of(field(v, "turns", &at)?, &at.key("turns"))?,
            active_at_end: seat_of(field(v, "active_at_end", &at)?, &at.key("active_at_end"))?,
            actions,
            redraws: redraw_flags,
            dut_cards: dut_cards_from_json(field(v, "dut_cards", &at)?, &at.key("dut_cards"))?,
            bench_seen: str_list(field(v, "bench_seen", &at)?, &at.key("bench_seen"))?,
            final_state_sha256: str_of(
                field(v, "final_state_sha256", &at)?,
                &at.key("final_state_sha256"),
            )?
            .into_string(),
            turn_limit: i32_of(field(v, "turn_limit", &at)?, &at.key("turn_limit"))?,
            max_actions: i64_of(field(v, "max_actions", &at)?, &at.key("max_actions"))?,
            decision_log,
        })
    }
}

impl MatchRecord {
    /// `MatchRecord.to_json` (`runner/records.py:176-184`): `games` lists the game indices
    /// and comes last.
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("match_index", self.match_index);
        o.set("dut_seat", self.dut_seat);
        o.set("winner", self.winner.as_str());
        o.set("dut_points", self.dut_points);
        o.set("bench_points", self.bench_points);
        o.set(
            "games",
            self.games
                .iter()
                .map(|g| g.game_index)
                .collect::<Vec<u32>>(),
        );
        o
    }
}

/// A decision-log dict (`ai/base.py:85-100`, `tests/runner/random_agent.py:30-40`) as a
/// [`LogEntry`]; absent `label` keys read as `None`. `value` must be a JSON float, as Python
/// writes `round(value, 4)` and `0.0`.
///
/// # Errors
/// [`SchemaError`] naming the first missing key or value of the wrong type.
pub fn log_entry_from_json(v: &Value) -> Result<LogEntry, SchemaError> {
    entry_from_json(v, &Path::root())
}

/// A JSON pointer under construction, for error messages.
#[derive(Clone)]
struct Path(String);

impl Path {
    const fn root() -> Self {
        Self(String::new())
    }

    fn key(&self, k: &str) -> Self {
        Self(format!(
            "{}/{}",
            self.0,
            k.replace('~', "~0").replace('/', "~1")
        ))
    }

    fn index(&self, i: usize) -> Self {
        Self(format!("{}/{i}", self.0))
    }

    fn error(&self, message: &str) -> SchemaError {
        SchemaError {
            pointer: self.0.clone(),
            message: message.to_owned(),
        }
    }
}

fn field<'a>(v: &'a Value, key: &str, at: &Path) -> Result<&'a Value, SchemaError> {
    match v {
        Value::Object(map) => map.get(key).ok_or_else(|| at.key(key).error("missing key")),
        _ => Err(at.error("expected an object")),
    }
}

fn arr<'a>(v: &'a Value, at: &Path) -> Result<&'a [Value], SchemaError> {
    v.as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| at.error("expected an array"))
}

fn u64_of(v: &Value, at: &Path) -> Result<u64, SchemaError> {
    v.as_u64()
        .ok_or_else(|| at.error("expected an integer in [0, 2**64)"))
}

fn i64_of(v: &Value, at: &Path) -> Result<i64, SchemaError> {
    v.as_i64()
        .ok_or_else(|| at.error("expected a 64-bit integer"))
}

fn i32_of(v: &Value, at: &Path) -> Result<i32, SchemaError> {
    v.as_i64()
        .and_then(|i| i32::try_from(i).ok())
        .ok_or_else(|| at.error("expected a 32-bit integer"))
}

fn u32_of(v: &Value, at: &Path) -> Result<u32, SchemaError> {
    v.as_u64()
        .and_then(|i| u32::try_from(i).ok())
        .ok_or_else(|| at.error("expected an integer in [0, 2**32)"))
}

fn seat_of(v: &Value, at: &Path) -> Result<u8, SchemaError> {
    v.as_u64()
        .and_then(|i| u8::try_from(i).ok())
        .filter(|&s| s < 2)
        .ok_or_else(|| at.error("expected seat 0 or 1"))
}

fn bool_of(v: &Value, at: &Path) -> Result<bool, SchemaError> {
    v.as_bool().ok_or_else(|| at.error("expected a boolean"))
}

fn str_of(v: &Value, at: &Path) -> Result<Box<str>, SchemaError> {
    v.as_str()
        .map(Box::from)
        .ok_or_else(|| at.error("expected a string"))
}

fn str_list(v: &Value, at: &Path) -> Result<Vec<Box<str>>, SchemaError> {
    arr(v, at)?
        .iter()
        .enumerate()
        .map(|(i, s)| str_of(s, &at.index(i)))
        .collect()
}

fn outcome_of(v: &Value, at: &Path) -> Result<Outcome, SchemaError> {
    let s = str_of(v, at)?;
    [Outcome::Dut, Outcome::Bench, Outcome::Draw]
        .into_iter()
        .find(|o| o.as_str() == &*s)
        .ok_or_else(|| at.error("expected 'dut', 'bench' or 'draw'"))
}

fn role_of(v: &Value, at: &Path) -> Result<Role, SchemaError> {
    let s = str_of(v, at)?;
    [Role::Dut, Role::Bench]
        .into_iter()
        .find(|r| r.as_str() == &*s)
        .ok_or_else(|| at.error("expected 'dut' or 'bench'"))
}

/// `Action.from_json` (`engine/state.py:26-28`): `[kind, a, b, c]`.
fn action_from_json(v: &Value, at: &Path) -> Result<Action, SchemaError> {
    let items = arr(v, at)?;
    if items.len() != 4 {
        return Err(at.error("expected [kind, a, b, c]"));
    }
    let kind = str_of(&items[0], &at.index(0))?;
    let kind = ActionKind::from_value(&kind)
        .ok_or_else(|| at.index(0).error("not an ActionKind value"))?;
    Ok(Action::new(
        kind,
        i32_of(&items[1], &at.index(1))?,
        i32_of(&items[2], &at.index(2))?,
        i32_of(&items[3], &at.index(3))?,
    ))
}

fn label_of(v: &Value, at: &Path) -> Result<Option<Box<str>>, SchemaError> {
    match v.get("label") {
        None => Ok(None),
        Some(l) => str_of(l, &at.key("label")).map(Some),
    }
}

fn entry_from_json(v: &Value, at: &Path) -> Result<LogEntry, SchemaError> {
    let decision_at = at.key("decision");
    let decision = str_of(field(v, "decision", at)?, &decision_at)?;
    let alts_at = at.key("alternatives");
    let alternatives = arr(field(v, "alternatives", at)?, &alts_at)?
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let p = alts_at.index(i);
            let value = field(a, "value", &p)?;
            if !value.is_f64() {
                return Err(p.key("value").error("expected a float"));
            }
            Ok(LogAlternative {
                action: action_from_json(field(a, "action", &p)?, &p.key("action"))?,
                label: label_of(a, &p)?,
                value: value.as_f64().unwrap_or(f64::NAN),
                visits: u32_of(field(a, "visits", &p)?, &p.key("visits"))?,
            })
        })
        .collect::<Result<Vec<LogAlternative>, SchemaError>>()?;
    Ok(LogEntry {
        turn: i32_of(field(v, "turn", at)?, &at.key("turn"))?,
        player: seat_of(field(v, "player", at)?, &at.key("player"))?,
        decision: DecisionKind::from_value(&decision)
            .ok_or_else(|| decision_at.error("not a DecisionKind value"))?,
        chosen: action_from_json(field(v, "chosen", at)?, &at.key("chosen"))?,
        label: label_of(v, at)?,
        alternatives,
    })
}

fn dut_cards_from_json(v: &Value, at: &Path) -> Result<DutCardEvents, SchemaError> {
    let played_at = at.key("played");
    let played = arr(field(v, "played", at)?, &played_at)?
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let p_at = played_at.index(i);
            let items = arr(p, &p_at)?;
            if items.len() != 3 {
                return Err(p_at.error("expected [card_number, turn, own_turn]"));
            }
            Ok(PlayEvent {
                card_number: str_of(&items[0], &p_at.index(0))?,
                turn: i32_of(&items[1], &p_at.index(1))?,
                own_turn: i32_of(&items[2], &p_at.index(2))?,
            })
        })
        .collect::<Result<Vec<PlayEvent>, SchemaError>>()?;
    Ok(DutCardEvents {
        initial_hand: str_list(field(v, "initial_hand", at)?, &at.key("initial_hand"))?,
        redrew: bool_of(field(v, "redrew", at)?, &at.key("redrew"))?,
        opening_hand: str_list(field(v, "opening_hand", at)?, &at.key("opening_hand"))?,
        drawn: str_list(field(v, "drawn", at)?, &at.key("drawn"))?,
        played,
        never_played: str_list(field(v, "never_played", at)?, &at.key("never_played"))?,
        in_hand_at_end: str_list(field(v, "in_hand_at_end", at)?, &at.key("in_hand_at_end"))?,
    })
}
