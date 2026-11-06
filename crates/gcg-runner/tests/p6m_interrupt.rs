//! W-106 (docs/dev/WEAKNESSES.md; PLAN §12.2 P6-M): games run on threads with a shared
//! cancellation flag, so an interrupt returns no results, every time, with no worker process
//! to reap. 200 runs, each interrupted by a real SIGINT while a match is being set up, at 1
//! and 4 workers. Its own test binary, so the SIGINT handler reaches no other test.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use gcg_ai::{Agent, AgentError};
use gcg_core::pool::Cancel;
use gcg_runner::{
    AgentFactory, AgentSpec, BenchmarkConfig, Format, RunError, SchemaVersion, TestAgentKind,
    Workers, agent_seed, run_benchmark,
};

fn deck(name: &str) -> gcg_data::Deck {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/decks");
    gcg_data::load_deck(&root.join(name)).expect("an example deck")
}

/// The random test agent; building match 2's first agent raises SIGINT at this process and
/// waits until the handler has set the run's flag, as a Ctrl-C during a run would.
struct InterruptingFactory {
    inner: Box<dyn AgentFactory>,
    trigger: u64,
    cancel: Cancel,
    sent: AtomicBool,
}

impl AgentFactory for InterruptingFactory {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        if spec.seed == self.trigger && !self.sent.swap(true, Ordering::SeqCst) {
            let pid = std::process::id().to_string();
            let status = std::process::Command::new("kill")
                .args(["-INT", &pid])
                .status()
                .expect("kill runs");
            assert!(status.success());
            while !self.cancel.is_cancelled() {
                std::hint::spin_loop();
            }
        }
        self.inner.make(spec)
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

#[test]
fn w106_two_hundred_interrupts_return_no_results() {
    let cfg = |workers: u32| BenchmarkConfig {
        deck_under_test: deck("red-green-zeon.txt"),
        benchmark_deck: deck("blue-white-federation.txt"),
        matches: 6,
        seed: 106,
        workers: Workers::Count(workers),
        format: Format::Bo1,
        ai_preset: "fast",
        out_dir: PathBuf::from("never-written"),
        decision_log: false,
        replays: 1,
        turn_limit: 200,
        max_actions: 20_000,
        schema: SchemaVersion::V1_1,
        package: "0.1.0",
        code_digest: "0000000000000000000000000000000000000000000000000000000000000000",
    };
    for i in 0..200 {
        let c = cfg(if i % 2 == 0 { 1 } else { 4 });
        let cancel = Cancel::new();
        gcg_core::sys::sigint::install(&cancel).expect("SIGINT handler");
        let f = InterruptingFactory {
            inner: gcg_runner::test_agents::factory(TestAgentKind::Random),
            trigger: agent_seed(c.seed, 2, 0, 0),
            cancel: cancel.clone(),
            sent: AtomicBool::new(false),
        };
        let mut calls = 0_u32;
        let result = run_benchmark(&c, &f, &mut |_, _| calls += 1, &cancel);
        assert_eq!(result.map(|_| ()), Err(RunError::Interrupted), "run {i}");
        assert!(calls < 6, "run {i}: {calls} matches reported");
    }
    assert!(!Path::new("never-written").exists());
}
