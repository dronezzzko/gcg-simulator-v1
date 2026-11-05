//! Data layer of gcg-sim: the card model and `CardDb` (cards.ndjson + overrides +
//! inline tokens + Unit variants, in def_id order), the official data (construction rules, the
//! Banned & Restricted list, rules and BO3 versions), deck parsing and validation with Python's
//! messages, `versions()`, conflict resolutions and the `data status` parts
//! (docs/dev/PLAN.md §2.1, §10.3.1).
//!
//! The contract types live in `contracts.rs` (lead-owned) and are re-exported here. Workstream
//! A implements the functions frozen in docs/dev/contracts.md.

mod contracts;
mod db;
mod deck;
mod embedded;
pub mod legality;
mod model;
pub mod names;
pub mod official;
mod py;
mod unicode;
mod validate;
mod versions;

pub use contracts::*;
pub use deck::{deck_digest, load_deck, parse_deck, resolve_card_number, to_decklist};
pub use py::py_path_str;
pub use unicode::py_repr;
pub use validate::{require_legal, validate};
pub use versions::{conflict_resolutions, data_status_parts, versions};
