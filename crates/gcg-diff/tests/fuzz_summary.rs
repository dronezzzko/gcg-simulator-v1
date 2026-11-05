//! The Rust fuzz runner's deterministic parts against Python's committed outputs: every fuzz trace
//! header's decks and game seed equal `deck_for` and `derive_seed(seed, "fuzz-game", index)`
//! (`tools/fuzz.py:47-87`), and the seed-2026 10,000-game summary
//! (`oracle/corpus/v1/tier1/fuzz/summary-2026-10000.json`, Python's `json.dumps(summary,
//! indent=1)`) is rebuilt byte for byte from its per-game rows, which `--compare` checks too.

use std::path::PathBuf;

use gcg_core::derive_seed;
use gcg_diff::fuzz::{GameOut, Pools, compare, summary_text};
use gcg_diff::read_trace;
use gcg_effects::Registry;

fn tier1() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/corpus/v1/tier1/fuzz"
    ))
}

#[test]
fn every_fuzz_trace_has_the_ported_decks_and_seed() {
    let reg = Registry::embedded();
    let pools = Pools::new(&reg);
    let mut n = 0;
    let mut files: Vec<PathBuf> = std::fs::read_dir(tier1())
        .expect("fuzz dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .collect();
    files.sort();
    for f in files {
        for t in read_trace(&f).expect("decodes") {
            // fuzz/s<seed>/g<index>
            let mut parts = t.header.id.split('/').skip(1);
            let seed: u64 = parts
                .next()
                .and_then(|s| s.strip_prefix('s'))
                .and_then(|s| s.parse().ok())
                .expect("seed");
            let index: i64 = parts
                .next()
                .and_then(|s| s.strip_prefix('g'))
                .and_then(|s| s.parse().ok())
                .expect("index");
            let decks = t.header.decks.as_ref().expect("decks");
            for (seat, d) in decks.iter().enumerate() {
                let mine = pools.deck_for(2 * index as u64 + seat as u64, seed);
                assert_eq!(mine.main, d.main, "{} seat {seat}", t.header.id);
                assert_eq!(mine.resources, d.resources, "{} seat {seat}", t.header.id);
            }
            assert_eq!(t.header.seed, Some(derive_seed!(seed, "fuzz-game", index)));
            n += 1;
        }
    }
    assert_eq!(n, 100);
}

#[test]
fn the_ten_thousand_game_summary_is_rebuilt_byte_for_byte() {
    let reg = Registry::embedded();
    let pools = Pools::new(&reg);
    let rows_text =
        std::fs::read_to_string(tier1().join("summary-2026-10000.games.jsonl")).expect("rows");
    let mut games = Vec::new();
    for line in rows_text.lines() {
        let r: serde_json::Value = serde_json::from_str(line).expect("row");
        let index = r["index"].as_u64().expect("index");
        let d0 = pools.deck_for(2 * index, 2026);
        let d1 = pools.deck_for(2 * index + 1, 2026);
        let mut cards: Vec<String> = d0
            .main
            .iter()
            .chain(&d1.main)
            .chain(&d0.resources)
            .chain(&d1.resources)
            .cloned()
            .collect();
        cards.sort();
        cards.dedup();
        let finished = r.get("actions").is_some();
        games.push(GameOut {
            index,
            seed: r["seed"].as_u64().expect("seed"),
            cards,
            ok: r["ok"].as_bool().expect("ok"),
            actions: r["actions"].as_u64().unwrap_or(0),
            end: r["end"].as_str().map(str::to_owned),
            turns: r["turns"].as_i64().and_then(|t| i32::try_from(t).ok()),
            error: r["error"].as_str().map(str::to_owned),
            last_actions: Vec::new(),
            finished,
        });
        assert_eq!(
            r["seed"].as_u64(),
            Some(derive_seed!(2026, "fuzz-game", index as i64))
        );
    }
    assert_eq!(games.len(), 10_000);
    let text = summary_text(&games, &pools);
    let want = std::fs::read_to_string(tier1().join("summary-2026-10000.json")).expect("summary");
    assert_eq!(text, want);
    compare(&text, &games, &tier1().join("summary-2026-10000.json")).expect("compares");
    // One changed game is found by its row.
    let mut changed = games.clone();
    changed[17].actions += 1;
    let err = compare(&text, &changed, &tier1().join("summary-2026-10000.json")).unwrap_err();
    assert!(err.starts_with("game 17 differs"), "{err}");
}
