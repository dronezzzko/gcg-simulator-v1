//! The decision log (`ai/base.py:79-122`): ranked, rounded entries built before the action is
//! applied, their JSON form, and the observer-safe action labels.

use std::cmp::Ordering;

use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::Json;
use gcg_engine::observe::is_hidden_from;
use gcg_engine::{Action, ActionKind, DecisionView, GameState, PLAYER_TARGET, Uid};

use crate::agent::uid_argument;
use crate::{Alternative, LOG_ALTERNATIVES, LogAlternative, LogEntry};

/// `_CARD_ARG_KINDS` (`ai/base.py:17-19`).
const CARD_ARG_KINDS: [ActionKind; 7] = [
    ActionKind::PlayUnit,
    ActionKind::PlayBase,
    ActionKind::Pair,
    ActionKind::PlayCommand,
    ActionKind::Activate,
    ActionKind::Attack,
    ActionKind::Block,
];

/// Python's `<` on the sort key `(-x.visits, -x.value)` (`ai/base.py:84`): tuples compare
/// element by element, the first unequal pair deciding with `<`.
fn ranks_before(x: &Alternative, y: &Alternative) -> bool {
    if x.visits != y.visits {
        return x.visits > y.visits;
    }
    let (a, b) = (-x.value, -y.value);
    #[expect(clippy::float_cmp, reason = "Python's tuple comparison uses == first")]
    let equal = a == b;
    !equal && a < b
}

/// `log_entry(st, player, chosen, alternatives)` (`ai/base.py:79-100`): `sorted` is stable, so
/// ties keep the agent's order; the top [`LOG_ALTERNATIVES`] are kept and rounded with
/// `round(value, 4)`.
///
/// # Panics
/// If `st` has no pending decision (`assert dec is not None`, `ai/base.py:83`); every caller
/// logs a decision it was asked to make.
#[must_use]
pub fn log_entry(st: &GameState, player: u8, chosen: Action, alts: &[Alternative]) -> LogEntry {
    let dec = st
        .pending()
        .expect("log_entry: a decision is pending (ai/base.py:83)");
    let mut ranked: Vec<&Alternative> = alts.iter().collect();
    ranked.sort_by(|x, y| {
        if ranks_before(x, y) {
            Ordering::Less
        } else if ranks_before(y, x) {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });
    ranked.truncate(LOG_ALTERNATIVES);
    LogEntry {
        turn: st.turn(),
        player,
        decision: dec.hdr.kind,
        chosen,
        label: Some(describe(st, &dec, chosen, player).into()),
        alternatives: ranked
            .into_iter()
            .map(|alt| LogAlternative {
                action: alt.action,
                label: Some(describe(st, &dec, alt.action, player).into()),
                value: round_f64(alt.value, 4),
                visits: alt.visits,
            })
            .collect(),
    }
}

impl LogEntry {
    /// The dict of `log_entry` (`ai/base.py:85-100`), keys in Python's insertion order; a
    /// `None` label writes no `label` key (the test agents' entries,
    /// `tests/runner/random_agent.py:30-40`).
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut out = Json::object();
        out.set("turn", self.turn);
        out.set("player", self.player);
        out.set("decision", self.decision.as_str());
        out.set("chosen", self.chosen.to_json());
        if let Some(label) = &self.label {
            out.set("label", &**label);
        }
        let alternatives = self
            .alternatives
            .iter()
            .map(|alt| {
                let mut a = Json::object();
                a.set("action", alt.action.to_json());
                if let Some(label) = &alt.label {
                    a.set("label", &**label);
                }
                a.set("value", alt.value);
                a.set("visits", alt.visits);
                a
            })
            .collect();
        out.set("alternatives", Json::Array(alternatives));
        out
    }
}

/// `card_label(st, uid, observer)` (`ai/base.py:103-109`): `player` for the player target,
/// `#uid` for a uid out of range or hidden from the observer, else `"{number} {name}"`.
#[must_use]
pub fn card_label(st: &GameState, uid: i32, observer: u8) -> String {
    if uid == PLAYER_TARGET {
        return "player".to_owned();
    }
    let in_range = usize::try_from(uid).is_ok_and(|u| u < st.cards().len());
    if !in_range {
        return format!("#{uid}");
    }
    let u = Uid::try_from(uid).expect("a uid below cards().len() fits in Uid");
    if is_hidden_from(st, u, observer) {
        return format!("#{uid}");
    }
    let cd = st.def_of(u);
    format!("{} {}", cd.card_number, cd.name)
}

/// `describe(st, dec, action, observer)` (`ai/base.py:112-122`): the readable form of `action`
/// using only what `observer` knows.
#[must_use]
pub fn describe(st: &GameState, dec: &DecisionView<'_>, action: Action, observer: u8) -> String {
    let k = action.kind;
    if CARD_ARG_KINDS.contains(&k) {
        let mut text = format!("{} {}", k.as_str(), card_label(st, action.a, observer));
        if matches!(k, ActionKind::Pair | ActionKind::Attack) {
            text.push_str(" -> ");
            text.push_str(&card_label(st, action.b, observer));
        }
        return text;
    }
    if k == ActionKind::Select && uid_argument(dec) {
        return format!("select {}", card_label(st, action.a, observer));
    }
    action.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alt(visits: u32, value: f64, c: i32) -> Alternative {
        Alternative {
            action: Action::new(ActionKind::Select, c, -1, -1),
            value,
            visits,
        }
    }

    /// `sorted(alts, key=lambda x: (-x.visits, -x.value))` is stable: equal keys, including
    /// `-0.0 == 0.0`, keep the agent's order.
    #[test]
    fn ranking_is_pythons_stable_sort() {
        let alts = [
            alt(3, 0.5, 0),
            alt(9, 0.1, 1),
            alt(3, 0.7, 2),
            alt(3, 0.5, 3),
            alt(0, -0.0, 4),
            alt(0, 0.0, 5),
            alt(9, 0.1, 6),
        ];
        let mut ranked: Vec<&Alternative> = alts.iter().collect();
        ranked.sort_by(|x, y| {
            if ranks_before(x, y) {
                Ordering::Less
            } else if ranks_before(y, x) {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        });
        let order: Vec<i32> = ranked.iter().map(|a| a.action.a).collect();
        assert_eq!(order, [1, 6, 2, 0, 3, 4, 5]);
    }

    #[test]
    fn entries_serialise_in_pythons_key_order() {
        let entry = LogEntry {
            turn: 4,
            player: 1,
            decision: gcg_engine::DecisionKind::Main,
            chosen: Action::new(ActionKind::PlayUnit, 62, -1, 0),
            label: Some("play_unit ST04-004 Moebius".into()),
            alternatives: vec![LogAlternative {
                action: Action::new(ActionKind::EndMain, -1, -1, -1),
                label: None,
                value: 0.4929,
                visits: 4,
            }],
        };
        let text = gcg_core::pyjson::dumps(
            &entry.to_json(),
            gcg_core::pyjson::DumpOptions {
                indent: None,
                sort_keys: false,
                compact: false,
                ensure_ascii: true,
            },
        );
        assert_eq!(
            text,
            "{\"turn\": 4, \"player\": 1, \"decision\": \"main\", \"chosen\": [\"play_unit\", 62, \
             -1, 0], \"label\": \"play_unit ST04-004 Moebius\", \"alternatives\": [{\"action\": \
             [\"end_main\", -1, -1, -1], \"value\": 0.4929, \"visits\": 4}]}"
        );
    }
}
