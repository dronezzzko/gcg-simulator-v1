//! W-04 (unit): the root-parallel decision and its statistics are bit-identical with 1, 4 and
//! 10 worker threads. W-11 (unit): the transposition, in-tree filter and subset mechanisms give
//! identical decisions and statistics across runs, thread counts and processes.

use std::num::NonZeroUsize;
use std::process::Command;
use std::sync::Mutex;

use gcg_core::pool::{Cancel, Pool};
use gcg_core::stable_hash::stable_key;
use gcg_engine::GameState;

use super::{Bits, config, search_in, search_inline, searched_states};

/// Every P6-D mechanism at once, with four root trees.
const BUNDLE: [(&str, &str); 6] = [
    ("root_trees", "4"),
    ("visited_set_backup", "true"),
    ("in_tree_filters", "true"),
    ("transpositions", "true"),
    ("macro_multi_pick", "true"),
    ("prior_worlds", "4"),
];

/// The searches of `states` with `flags` as tasks of a pool of `threads` workers.
fn on_pool(threads: usize, states: &[(GameState, u8, u64)], flags: &[(&str, &str)]) -> Vec<Bits> {
    let cfg = config(flags);
    let cancel = Cancel::new();
    let inputs: Vec<Mutex<Option<(GameState, u8, u64)>>> =
        states.iter().map(|s| Mutex::new(Some(s.clone()))).collect();
    let pool = Pool::new(NonZeroUsize::new(threads).expect("threads >= 1"));
    pool.map_indexed(
        inputs.len(),
        &cancel,
        |i, scratch, par| {
            let (st, player, seed) = inputs[i]
                .lock()
                .expect("not poisoned")
                .take()
                .expect("each task once");
            search_in(scratch, par, &cancel, &st, player, &cfg, seed)
        },
        |_| {},
    )
    .unwrap_or_else(|e| panic!("search failed: {e:?}"))
}

#[test]
fn root_trees_give_the_same_decision_for_any_worker_count() {
    let states = searched_states(8);
    for flags in [&[("root_trees", "4")][..], &[("root_trees", "8")], &BUNDLE] {
        let serial: Vec<Bits> = states
            .iter()
            .map(|(st, p, seed)| search_inline(st, *p, &config(flags), *seed))
            .collect();
        for threads in [1, 4, 10] {
            assert_eq!(
                on_pool(threads, &states, flags),
                serial,
                "{flags:?} {threads}"
            );
        }
    }
}

#[test]
fn root_trees_split_the_budget_and_draw_one_seed() {
    let null = config(&[]);
    for (st, p, seed) in searched_states(12) {
        let reference = search_inline(&st, p, &null, seed);
        let mut after = None;
        for k in [1u32, 2, 4, 8] {
            let text = k.to_string();
            let cfg = config(&[("root_trees", &text)]);
            let (_, alternatives, stats, _, rng) = search_inline(&st, p, &cfg, seed);
            // The root moves and the decision's budget are the reference's.
            assert_eq!(stats.roots, reference.2.roots);
            assert_eq!(
                (stats.budget, stats.cap),
                (reference.2.budget, reference.2.cap)
            );
            if stats.budget == 0 {
                continue;
            }
            // Each tree gets ceil(B/k) iterations and at most ceil(cap/k).
            assert!(
                stats.iterations <= k * stats.cap.div_ceil(k),
                "{k}: {stats:?}"
            );
            assert!(stats.effort.worlds >= u64::from(k));
            let visits: u32 = alternatives.iter().map(|a| a.2).sum();
            assert_eq!(visits, stats.iterations);
            // After the root moves the ensemble draws one seed, whatever k is.
            assert_eq!(*after.get_or_insert(rng), rng, "{k}");
        }
    }
}

#[test]
fn every_mechanism_repeats_across_runs_and_thread_counts() {
    let states = searched_states(6);
    for flags in [
        &[("transpositions", "true")][..],
        &[("in_tree_filters", "true")],
        &[("macro_multi_pick", "true")],
        &[("prior_worlds", "4")],
        &[("visited_set_backup", "true")],
        &BUNDLE,
    ] {
        let first = on_pool(1, &states, flags);
        assert_eq!(on_pool(1, &states, flags), first, "{flags:?}");
        assert_eq!(on_pool(4, &states, flags), first, "{flags:?}");
    }
}

/// The digest of the bundle's searches on the first vector decisions (`Debug` of the bits).
fn bundle_digest() -> String {
    let results: Vec<Bits> = searched_states(4)
        .iter()
        .map(|(st, p, seed)| search_inline(st, *p, &config(&BUNDLE), *seed))
        .collect();
    format!("{:016x}", stable_key(format!("{results:?}").as_bytes()).0)
}

/// Prints the bundle digest when run as the child of
/// `the_bundle_repeats_in_another_process`.
#[test]
fn bundle_digest_child() {
    if std::env::var_os("P6D_DIGEST_CHILD").is_some() {
        println!("P6D-DIGEST {}", bundle_digest());
    }
}

#[test]
fn the_bundle_repeats_in_another_process() {
    let exe = std::env::current_exe().expect("the test binary");
    let out = Command::new(exe)
        .args([
            "--exact",
            "p6d_determinism::bundle_digest_child",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .env("P6D_DIGEST_CHILD", "1")
        .output()
        .expect("the child process runs");
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let child = stdout
        .lines()
        .find_map(|l| l.split("P6D-DIGEST ").nth(1)?.split_whitespace().next())
        .unwrap_or_else(|| panic!("no digest in {stdout}"));
    assert_eq!(child, bundle_digest());
}
