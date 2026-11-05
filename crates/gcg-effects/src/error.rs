//! `Display` and `Error` for [`IrError`]: every message names what to do (PLAN §3.4).

use std::fmt;

use crate::contracts::{IR_REGENERATE_COMMAND, IrError, SUPPORTED_IR_VERSION};

fn card(card_number: Option<&String>) -> String {
    card_number.map_or_else(String::new, |n| format!(" in the script of {n}"))
}

impl fmt::Display for IrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json { message } => write!(f, "the effect IR is not valid JSON: {message}"),
            Self::Version { found } => write!(
                f,
                "effect IR version {found} is not supported (this build reads version \
                 {SUPPORTED_IR_VERSION}); regenerate it with `{IR_REGENERATE_COMMAND}`"
            ),
            Self::Schema { detail } => write!(
                f,
                "the effect IR schema differs from this build's DSL types ({detail}); \
                 regenerate the IR with `{IR_REGENERATE_COMMAND}` and the types with \
                 `uv run python port/oracle/gen_dsl.py --write`"
            ),
            Self::UnknownClass { class, card_number } => write!(
                f,
                "unknown DSL class {class:?}{}; regenerate the DSL types with \
                 `uv run python port/oracle/gen_dsl.py --write`",
                card(card_number.as_ref())
            ),
            Self::UnknownHook {
                kind,
                name,
                card_number,
            } => write!(
                f,
                "unknown {kind} hook {name:?}{}: the Rust engine has no body for it",
                card(card_number.as_ref())
            ),
            Self::Malformed { path, detail } => {
                write!(f, "malformed effect IR at {path}: {detail}")
            }
            Self::DataMismatch {
                which,
                expected,
                found,
            } => write!(
                f,
                "the effect IR was exported from other card data ({which}: the IR has \
                 {expected}, this build {found}); regenerate it with `{IR_REGENERATE_COMMAND}`"
            ),
        }
    }
}

impl std::error::Error for IrError {}
