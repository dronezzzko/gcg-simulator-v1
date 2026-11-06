//! Operating-system glue in safe code only: CPU topology from `sysctl` (A4) and SIGINT through
//! `signal-hook` (A3). Workstream E owns the bodies (docs/dev/PLAN.md §10.1).

pub mod sigint;
pub mod topology;
