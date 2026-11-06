//! The replay document (`replay_document`, `runner/replayfile.py:104-150`), assembled from a
//! game record and the result of re-simulating it, and the messages of [`ReplayError`].
//!
//! Python re-simulates the record's actions (`replay_with_log`, `:75-101`) to get the readable
//! log and `winner_seat`; `turns` and `final_state_sha256` are copied from the record, and
//! nothing compares the re-simulation with the record at write time. The
//! re-simulation needs the engine and lives in the runner's `replay` module; this module only
//! builds the document from its [`Resimulation`].

use std::fmt;

use gcg_core::pyjson::{Json, Key};
use gcg_data::{CardCount, Deck};

use crate::records::{actions_json, decision_log_json};
use crate::{BenchmarkConfig, GameRecord, REPLAY_FORMAT, REPLAY_VERSION, ReplayError, Role};

/// `str(exc)` of the `ReplayError` each variant stands for (`runner/replayfile.py`), which the
/// CLI prints as `f"cannot replay {path}: {exc}"` (`cli.py:353-356`). `Engine` is an engine
/// exception Python does not catch; it prints the engine's message.
impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // `:166-167`
            Self::NotReplay => write!(f, "not a {REPLAY_FORMAT} v{REPLAY_VERSION} document"),
            // `:86-87`
            Self::Unimplemented(e) => {
                write!(f, "the replay uses a card this version cannot play: {e}")
            }
            // `:92-93`
            Self::ExtraActions => {
                f.write_str("the replay has more actions than the game had decisions")
            }
            // `:97-98`; `{action}` is `Action.__str__` (`engine/state.py:30-32`).
            Self::Illegal { index, action } => {
                write!(
                    f,
                    "action {index} {action} is not legal in the replayed game"
                )
            }
            // `:180-183`
            Self::Malformed { py_class, detail } => {
                write!(f, "malformed replay document: {py_class}: {detail}")
            }
            Self::Engine(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unimplemented(e) => Some(e),
            Self::Engine(e) => Some(e),
            Self::NotReplay
            | Self::ExtraActions
            | Self::Illegal { .. }
            | Self::Malformed { .. } => None,
        }
    }
}

/// What `replay_with_log` returns besides the state: `st.winner` after the last action (`-1`
/// for a draw, `None` for an unfinished game) and the log lines, the last one
/// `"T{turn} game over: winner P{winner} ({reason})"` (`runner/replayfile.py:94-100`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Resimulation {
    pub winner_seat: Option<i8>,
    pub log: Vec<String>,
}

/// `dict(sorted(Counter(cards).items()))` (`runner/replayfile.py:135-136`); a deck's
/// run-length lists are already sorted by card number.
fn counts(cards: &[CardCount]) -> Json {
    Json::Object(
        cards
            .iter()
            .map(|cc| {
                let n = i128::try_from(cc.count).expect("a deck's copy count fits in i128");
                (Key::Str(cc.card_number.to_string()), Json::Int(n))
            })
            .collect(),
    )
}

/// `replay_document(record, config, label)` (`runner/replayfile.py:104-150`) with the
/// re-simulation's winner seat and log. Seat decks follow `seat_decks(config,
/// record.match_index)` (`runner/config.py:69-72`: the deck under test sits in seat
/// `match_index % 2`), and deck names follow `record.dut_seat`, as Python's `names` does.
#[must_use]
pub fn replay_document_with(
    record: &GameRecord,
    cfg: &BenchmarkConfig,
    label: &str,
    sim: &Resimulation,
) -> Json {
    let (dut, bench) = (&cfg.deck_under_test, &cfg.benchmark_deck);
    let decks: [&Deck; 2] = if record.match_index.is_multiple_of(2) {
        [dut, bench]
    } else {
        [bench, dut]
    };
    let seats: Vec<Json> = (0u8..2)
        .map(|seat| {
            let role = if seat == record.dut_seat {
                Role::Dut
            } else {
                Role::Bench
            };
            let name = if seat == record.dut_seat {
                &dut.name
            } else {
                &bench.name
            };
            let i = usize::from(seat);
            let mut s = Json::object();
            s.set("seat", seat);
            s.set("role", role.as_str());
            s.set("deck", &**name);
            s.set("agent", &*record.agents[i]);
            s.set("agent_seed", record.agent_seeds[i]);
            s.set("main", counts(&decks[i].main));
            s.set("resources", counts(&decks[i].resources));
            s
        })
        .collect();
    let mut expected = Json::object();
    expected.set("winner", record.winner.as_str());
    expected.set("winner_seat", sim.winner_seat);
    expected.set("end_reason", record.end_reason.as_str());
    expected.set("turns", record.turns);
    expected.set("final_state_sha256", record.final_state_sha256.as_str());
    let mut o = Json::object();
    o.set("format", REPLAY_FORMAT);
    o.set("version", REPLAY_VERSION);
    o.set("label", label);
    o.set("match_index", record.match_index);
    o.set("game_index", record.game_index);
    o.set("seed", record.seed);
    o.set("chooser", record.chooser);
    o.set("turn_limit", record.turn_limit);
    o.set("max_actions", record.max_actions);
    o.set("seats", Json::Array(seats));
    o.set("actions", actions_json(&record.actions));
    o.set("expected", expected);
    o.set(
        "log",
        Json::Array(sim.log.iter().map(|l| Json::Str(l.clone())).collect()),
    );
    o.set(
        "decision_log",
        decision_log_json(record.decision_log.as_deref()),
    );
    o
}

#[cfg(test)]
mod tests {
    use gcg_engine::{ActionKind, EngineError, UnimplementedCardError};

    use super::*;

    #[test]
    fn replay_errors_print_pythons_messages() {
        let unimplemented = UnimplementedCardError {
            card_number: "GD01-001".into(),
            reason: "no program".into(),
        };
        let cases = [
            (ReplayError::NotReplay, "not a gcg-sim-replay v1 document"),
            (
                ReplayError::Unimplemented(unimplemented.clone()),
                "the replay uses a card this version cannot play: GD01-001: no program",
            ),
            (
                ReplayError::ExtraActions,
                "the replay has more actions than the game had decisions",
            ),
            (
                ReplayError::Illegal {
                    index: 4,
                    action: gcg_engine::Action::new(ActionKind::Attack, 5, -1, -1),
                },
                "action 4 attack(5) is not legal in the replayed game",
            ),
            (
                ReplayError::Malformed {
                    py_class: "KeyError",
                    detail: "'seats'".to_owned(),
                },
                "malformed replay document: KeyError: 'seats'",
            ),
            (
                ReplayError::Engine(EngineError::Internal("broken".to_owned())),
                "broken",
            ),
        ];
        for (error, text) in cases {
            assert_eq!(error.to_string(), text);
        }
        let error = ReplayError::Unimplemented(unimplemented);
        let source = std::error::Error::source(&error);
        assert_eq!(
            source.map(ToString::to_string).as_deref(),
            Some("GD01-001: no program")
        );
        assert!(std::error::Error::source(&ReplayError::ExtraActions).is_none());
    }
}
