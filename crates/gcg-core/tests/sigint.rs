//! `gcg_core::sys::sigint::install` (PLAN A3, §7.5): after installation SIGINT sets the
//! `Cancel` flag instead of ending the process. Its own test binary, so the handler never
//! reaches another test's process.

use gcg_core::pool::Cancel;
use gcg_core::sys::sigint;

#[test]
fn sigint_sets_the_cancel_flag_and_does_not_end_the_process() {
    let cancel = Cancel::new();
    let observer = cancel.clone();
    sigint::install(&cancel).expect("SIGINT handler installed");
    assert!(!observer.is_cancelled());
    // raise() delivers the signal to this thread before it returns.
    signal_hook::low_level::raise(signal_hook::consts::SIGINT).expect("raise SIGINT");
    assert!(observer.is_cancelled());
    assert!(cancel.is_cancelled());
}
