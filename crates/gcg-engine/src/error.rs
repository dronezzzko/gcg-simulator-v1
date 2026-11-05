//! `Display`, `std::error::Error` and the `From` conversions of the engine's error types
//! (docs/dev/contracts.md, gcg-engine "error.rs"). `Display` prints `str(exc)` of the Python
//! exception each error stands for, so `f"{type(exc).__name__}: {exc}"`
//! (`runner/game.py:131-136`) is `format!("{}: {err}", err.py_class())`. Where Python's text
//! embeds CPython exception text, only the prefix is Python's (PLAN A19).

use std::error::Error;
use std::fmt;

use crate::{
    ApplyError, CanonError, CheckError, DeckConstructionError, EngineError, IllegalActionError,
    InvariantViolation, NewGameError, UnimplementedCardError,
};

/// `str(UnimplementedCardError(f"{e.card_number}: {e.error}"))` (`effects/registry.py:143-147`).
impl fmt::Display for UnimplementedCardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.card_number, self.reason)
    }
}

impl Error for UnimplementedCardError {}

/// `str(exc)`: the message Python raised with. `Key(key)` prints `repr(key)`, as
/// `str(KeyError(key))` does (`cards/db.py:299-303`). The variants without a Python
/// counterpart (W-143, W-47, PLAN §4.1 item 6, §4.3 "Capacities") name what went wrong.
impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal(m) | Self::Value(m) | Self::Index(m) | Self::Assertion(m) => {
                f.write_str(m)
            }
            // `str(KeyError(key))` is `repr(key)`, CPython's exactly (gcg-data's tables).
            Self::Key(key) => f.write_str(&gcg_data::py_repr(key)),
            Self::Unimplemented(e) => e.fmt(f),
            Self::RedirectCycle { uids } => {
                f.write_str("battle damage redirect cycle through units [")?;
                for (i, u) in uids.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{u}")?;
                }
                f.write_str("]")
            }
            Self::Overflow { what } => write!(f, "integer overflow in {what}"),
            Self::Capacity { what, capacity } => {
                write!(f, "{what} is full (capacity {capacity})")
            }
            Self::NoProgress { steps } => {
                write!(f, "advance made no progress in {steps} steps")
            }
        }
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Unimplemented(e) => Some(e),
            _ => None,
        }
    }
}

/// `"no decision pending"` / `f"illegal action {action} for {dec.kind.value}"`
/// (`engine/game.py:783-786`; `str(action)` is `Action.__str__`, `state.py:30-32`).
impl fmt::Display for IllegalActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDecision => f.write_str("no decision pending"),
            Self::Illegal { action, kind } => {
                write!(f, "illegal action {action} for {}", kind.as_str())
            }
        }
    }
}

impl Error for IllegalActionError {}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Illegal(e) => e.fmt(f),
            Self::Engine(e) => e.fmt(f),
        }
    }
}

impl Error for ApplyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Illegal(e) => Some(e),
            Self::Engine(e) => Some(e),
        }
    }
}

impl From<EngineError> for ApplyError {
    fn from(e: EngineError) -> Self {
        Self::Engine(e)
    }
}

impl From<IllegalActionError> for ApplyError {
    fn from(e: IllegalActionError) -> Self {
        Self::Illegal(e)
    }
}

/// `f"player {p}: " + "; ".join(problems)` (`engine/game.py:124-127`).
impl fmt::Display for DeckConstructionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "player {}: {}", self.player, self.problems.join("; "))
    }
}

impl Error for DeckConstructionError {}

impl fmt::Display for NewGameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Construction(e) => e.fmt(f),
            Self::Unimplemented(e) => e.fmt(f),
            Self::Engine(e) => e.fmt(f),
        }
    }
}

impl Error for NewGameError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Construction(e) => Some(e),
            Self::Unimplemented(e) => Some(e),
            Self::Engine(e) => Some(e),
        }
    }
}

impl From<EngineError> for NewGameError {
    fn from(e: EngineError) -> Self {
        Self::Engine(e)
    }
}

/// Rust-only (snapshot loading has no Python counterpart): the JSON path and what is wrong.
impl fmt::Display for CanonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape { path, detail } => write!(f, "malformed snapshot at {path}: {detail}"),
            Self::UnknownContent { kind, id } => write!(f, "unknown {kind} content id {id}"),
            Self::Value { path, detail } => write!(f, "invalid value at {path}: {detail}"),
        }
    }
}

impl Error for CanonError {}

/// `"; ".join(errors[:10])` (`engine/invariants.py:108-109`).
impl fmt::Display for InvariantViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.errors.iter().take(10).enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            f.write_str(e)?;
        }
        Ok(())
    }
}

impl Error for InvariantViolation {}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Violation(e) => e.fmt(f),
            Self::Engine(e) => e.fmt(f),
        }
    }
}

impl Error for CheckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Violation(e) => Some(e),
            Self::Engine(e) => Some(e),
        }
    }
}

impl From<EngineError> for CheckError {
    fn from(e: EngineError) -> Self {
        Self::Engine(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, ActionKind, DecisionKind, PLAYER_TARGET};

    #[test]
    fn messages_are_pythons() {
        let illegal = IllegalActionError::Illegal {
            action: Action::new(ActionKind::Attack, 5, PLAYER_TARGET, -1),
            kind: DecisionKind::Main,
        };
        assert_eq!(illegal.to_string(), "illegal action attack(5,-2) for main");
        assert_eq!(
            ApplyError::from(IllegalActionError::NoDecision).to_string(),
            "no decision pending"
        );
        let deck = DeckConstructionError {
            player: 1,
            problems: vec!["deck has 49 cards (need 50)".into(), "x".into()],
        };
        assert_eq!(deck.to_string(), "player 1: deck has 49 cards (need 50); x");
        let unimpl = UnimplementedCardError {
            card_number: "GD01-001".into(),
            reason: "no script".into(),
        };
        assert_eq!(unimpl.to_string(), "GD01-001: no script");
        assert_eq!(
            NewGameError::Unimplemented(unimpl.clone()).to_string(),
            "GD01-001: no script"
        );
        let e = EngineError::Unimplemented(unimpl);
        assert_eq!(
            format!("{}: {e}", e.py_class()),
            "UnimplementedCardError: GD01-001: no script"
        );
        assert!(e.source().is_some());
    }

    #[test]
    fn key_errors_print_the_repr_of_the_key() {
        // str(KeyError('GD01-999')) == "'GD01-999'"; str(KeyError("it's")) == '"it\'s"'.
        assert_eq!(
            EngineError::Key("GD01-999".into()).to_string(),
            "'GD01-999'"
        );
        assert_eq!(EngineError::Key("it's".into()).to_string(), "\"it's\"");
        // Non-ASCII: printable characters are kept, others escaped (CPython unicode_repr).
        assert_eq!(
            EngineError::Key("Zaku \u{2161}\u{200b}".into()).to_string(),
            "'Zaku \u{2161}\\u200b'"
        );
        let e = EngineError::Value("cannot determinize: empty pool for hidden cards".into());
        assert_eq!(
            format!("{}: {e}", e.py_class()),
            "ValueError: cannot determinize: empty pool for hidden cards"
        );
    }

    #[test]
    fn invariant_violations_join_the_first_ten() {
        let v = InvariantViolation {
            errors: (0..12).map(|i| format!("e{i}")).collect(),
        };
        assert_eq!(v.to_string(), "e0; e1; e2; e3; e4; e5; e6; e7; e8; e9");
        assert_eq!(
            CheckError::Violation(v).to_string().matches("; ").count(),
            9
        );
    }

    #[test]
    fn rust_only_errors_name_their_cause() {
        assert_eq!(
            EngineError::RedirectCycle {
                uids: Box::new([3, 9])
            }
            .to_string(),
            "battle damage redirect cycle through units [3, 9]"
        );
        assert_eq!(
            EngineError::Capacity {
                what: "event payload",
                capacity: 10
            }
            .to_string(),
            "event payload is full (capacity 10)"
        );
        assert_eq!(
            CanonError::UnknownContent {
                kind: "program",
                id: "0123456789abcdef".into()
            }
            .to_string(),
            "unknown program content id 0123456789abcdef"
        );
    }
}
