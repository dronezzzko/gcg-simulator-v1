//! SIGINT handling (docs/port/PLAN.md A3, §7.5): `install(&Cancel)` registers the cancel flag
//! with `signal_hook::flag::register(SIGINT, ..)`, which stores `true` on SIGINT. The module is
//! safe code; the coordinator then stops dispatch, joins the workers, writes no report, prints
//! `interrupted: workers stopped, no reports written` and exits 130 (`cli.py:397-399`).
//!
//! The function lands with workstream E; its frozen signature is in docs/port/contracts.md
//! (gcg-core section). It needs no contract type beyond [`crate::pool::Cancel`].
