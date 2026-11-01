//! Data layer of the gcg-sim port: the card model and `CardDb` (cards.ndjson + overrides +
//! inline tokens + Unit variants, in def_id order), the official data (construction rules, the
//! Banned & Restricted list, rules and BO3 versions), deck parsing and validation with Python's
//! messages, `versions()`, conflict resolutions and the `data status` parts
//! (docs/port/PLAN.md §2.1, §10.3.1; docs/port/inventory/03-data-deck.md).
//!
//! The contract types live in `contracts.rs` (lead-owned) and are re-exported here. Workstream
//! A implements the functions frozen in docs/port/contracts.md.

mod contracts;

pub use contracts::*;
