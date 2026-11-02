//! Long Phase 6 checks (feature `slow`, PLAN A12): W-150 and W-152 of P6-I (PLAN §12.2).
//!
//! - **W-150.** The `fast` and `standard` presets, reference (`reference-benchmark`) and
//!   strengthened (`preset-benchmark` with the shipped configurations), and a head-to-head of
//!   the strengthened against the reference search, at 1, 4 and 10 workers on two pairings:
//!   every deterministic output is byte-identical (`timing.json` is wall-clock only and is
//!   not compared). Negative control: an agent that breaks ties by iterating a randomly
//!   seeded std `HashMap` makes the same comparison fail, while the random agent passes it.
//! - **W-152.** Mirror-deck games (the same decklist in both seats, the agents choosing who
//!   goes first): 4,000 random-agent games and 400 `search:fast` games. The seat-0 win rate's
//!   Wilson 95% interval contains 0.5 and the first player's win rate does not differ by seat
//!   (two-proportion z-test, alpha = 0.01). Negative control: the same harness with two extra
//!   Shields for seat 0 (two deck cards moved to its Shield area before setup ends) fails the
//!   seat check.

use std::path::{Path, PathBuf};
use std::process::Command;

use gcg_ai::{Agent, AgentCx, AgentError, LogEntry};
use gcg_core::derive_seed;
use gcg_core::pool::Cancel;
use gcg_data::DeckList;
use gcg_effects::Registry;
use gcg_engine::{Action, GameState, NewGameOptions, Zone, apply, new_game};
use gcg_eval::agent_spec::{make_agent, parse_agent_spec};
use gcg_eval::selfplay::{GameError, run_indexed};
use gcg_eval::{AgentDesc, DECISION_LIMIT};
use gcg_runner::Workers;
use gcg_runner::reports::stats::{Z95, wilson};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("phase6-slow-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn run_ok(args: &[&str]) {
    let o = Command::new(env!("CARGO_BIN_EXE_gcg-eval"))
        .args(args)
        .current_dir(root())
        .env_remove("COLUMNS")
        .output()
        .expect("gcg-eval runs");
    assert_eq!(
        o.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
}

/// Every deterministic file of a report directory (or a single report file), by relative
/// path, with the directory's own path masked where a report prints it.
fn outputs(path: &Path) -> Vec<(String, Vec<u8>)> {
    if path.is_file() {
        return vec![(String::new(), std::fs::read(path).expect("report"))];
    }
    let mut out = Vec::new();
    let mut stack = vec![path.to_owned()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("dir").filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n != "timing.json") {
                let rel = p
                    .strip_prefix(path)
                    .expect("inside")
                    .to_string_lossy()
                    .into_owned();
                let text = std::fs::read(&p).expect("file");
                let masked = String::from_utf8_lossy(&text)
                    .replace(path.to_str().expect("UTF-8"), "OUT")
                    .into_bytes();
                out.push((rel, masked));
            }
        }
    }
    out.sort();
    out
}

/// W-150: runs `args` with `--out DIR` (or `--out FILE`) at 1, 4 and 10 workers and asserts
/// that every deterministic output is byte-identical.
fn identical_at_1_4_10(args: &[&str], dir: &Path, name: &str, file: bool) {
    let mut seen: Vec<Vec<(String, Vec<u8>)>> = Vec::new();
    for w in ["1", "4", "10"] {
        let out = dir.join(format!("{name}-{w}{}", if file { ".json" } else { "" }));
        let mut a = args.to_vec();
        let out_s = out.to_str().expect("UTF-8").to_owned();
        a.extend(["--workers", w, "--out", &out_s]);
        run_ok(&a);
        seen.push(outputs(&out));
    }
    assert!(!seen[0].is_empty(), "{name}: no outputs");
    for (k, w) in [(1, "4"), (2, "10")] {
        assert_eq!(
            seen[0].iter().map(|(n, _)| n).collect::<Vec<_>>(),
            seen[k].iter().map(|(n, _)| n).collect::<Vec<_>>(),
            "{name}: files at 1 vs {w} workers"
        );
        for ((n, a), (_, b)) in seen[0].iter().zip(&seen[k]) {
            assert!(a == b, "{name}: {n} differs at 1 vs {w} workers");
        }
    }
}

#[test]
fn w150_every_preset_is_identical_at_1_4_and_10_workers() {
    let dir = scratch("w150");
    let mut configs = Vec::new();
    for p in gcg_search::Preset::ALL {
        let file = dir.join(format!("{}.json", p.as_str()));
        std::fs::write(
            &file,
            gcg_core::pyjson::dumps(
                &gcg_eval::search_config::to_json(gcg_search::preset_config(p)),
                gcg_core::pyjson::DumpOptions::PRETTY_SORTED_UTF8,
            ),
        )
        .expect("config");
        configs.push((p.as_str(), file));
    }
    let pairings = [
        (
            "examples/decks/red-green-zeon.txt",
            "examples/decks/blue-white-federation.txt",
        ),
        (
            "examples/decks/purple-white-tekkadan.txt",
            "examples/decks/red-green-zeon.txt",
        ),
    ];
    for (k, (dut, bench)) in pairings.iter().enumerate() {
        let common = [
            *dut,
            *bench,
            "--matches",
            "2",
            "--format",
            "bo1",
            "--seed",
            "11",
            "--decision-log",
            "--replays",
            "2",
        ];
        for preset in ["fast", "standard"] {
            let mut a = vec!["reference-benchmark"];
            a.extend(common);
            a.extend(["--reference-preset", preset]);
            identical_at_1_4_10(&a, &dir, &format!("ref-{preset}-{k}"), false);
        }
        for (preset, file) in &configs {
            let mut a = vec!["preset-benchmark"];
            a.extend(common);
            a.extend([
                "--ai-preset",
                preset,
                "--search-config",
                file.to_str().expect("UTF-8"),
            ]);
            identical_at_1_4_10(&a, &dir, &format!("search-{preset}-{k}"), false);
        }
    }
    identical_at_1_4_10(
        &[
            "head-to-head",
            "search:fast",
            "ref:standard",
            "--pair",
            "red-green-zeon:blue-white-federation,purple-white-tekkadan:red-green-zeon",
            "--games",
            "8",
            "--seed",
            "12",
        ],
        &dir,
        "h2h",
        true,
    );
}

/// The W-150 negative control: ties broken by the iteration order of a randomly seeded std
/// `HashMap`, which differs between maps.
struct HashTieAgent;

impl Agent for HashTieAgent {
    fn name(&self) -> &str {
        "hash-tie"
    }

    #[expect(
        clippy::disallowed_types,
        reason = "the W-150 negative control needs a randomly seeded HashMap"
    )]
    fn choose(
        &mut self,
        st: &GameState,
        player: u8,
        _cx: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError> {
        let dec = st
            .pending()
            .filter(|d| d.hdr.player == player)
            .ok_or_else(|| AgentError::Value("no decision".to_owned()))?;
        let options: std::collections::HashMap<usize, Action> =
            dec.options.iter().copied().enumerate().collect();
        options
            .values()
            .next()
            .copied()
            .ok_or_else(|| AgentError::Value("no option".to_owned()))
    }

    fn decision_log(&self) -> &[LogEntry] {
        &[]
    }
}

/// The chosen actions of `games` games, the agents built by `make`.
fn action_lists(
    games: u32,
    make: &(dyn Fn(u64) -> Box<dyn Agent> + Sync),
    workers: u32,
) -> Vec<Vec<Action>> {
    let reg = Registry::embedded();
    let decks = gcg_eval::decks::embedded(gcg_eval::DeckSet::Tuning).expect("tuning");
    run_indexed(
        usize::try_from(games).unwrap_or(0),
        Workers::Count(workers),
        &Cancel::new(),
        &mut |_| {},
        |i, cx| {
            let seed = derive_seed!(5, "hash", i);
            let mut st = new_game(
                &reg,
                [&decks[0].deck, &decks[1].deck],
                seed,
                NewGameOptions::default(),
            )?;
            let mut agents = [make(seed), make(seed ^ 1)];
            let mut actions = Vec::new();
            while actions.len() < 2000 {
                let Some(p) = st.pending().map(|d| d.hdr.player) else {
                    break;
                };
                let a = agents[usize::from(p)]
                    .choose(&st, p, cx)
                    .map_err(GameError::agent)?;
                apply(&mut st, a, true)?;
                actions.push(a);
            }
            Ok(actions)
        },
        |i| (u32::try_from(i).unwrap_or(0), 0),
    )
    .expect("games")
}

#[test]
fn w150_negative_control_hash_order_breaks_the_comparison() {
    let random = |seed: u64| gcg_ai::make_random_agent(seed, false);
    assert_eq!(
        action_lists(6, &random, 1),
        action_lists(6, &random, 4),
        "random is deterministic"
    );
    let hash = |_: u64| -> Box<dyn Agent> { Box::new(HashTieAgent) };
    assert_ne!(
        action_lists(6, &hash, 1),
        action_lists(6, &hash, 1),
        "the comparison detects hash-order tie-breaking"
    );
}

/// One mirror game: `(winner, first player)`; with `handicap`, seat 0 gets two extra Shields.
fn mirror_game(
    reg: &std::sync::Arc<Registry>,
    agent: &AgentDesc,
    deck: &DeckList,
    seed: u64,
    handicap: bool,
    cx: &mut AgentCx<'_>,
) -> Result<(i8, u8), GameError> {
    let mut st = new_game(reg, [deck, deck], seed, NewGameOptions::default())?;
    if handicap {
        let top: Vec<_> = gcg_engine::testkit::uids_in(&st, 0, Zone::Deck)
            .into_iter()
            .take(2)
            .collect();
        for uid in top {
            gcg_engine::testkit::engine::move_card(&mut st, uid, Zone::Shield, false, false, false)
                .map_err(|e| GameError::engine(&e))?;
        }
    }
    let mut agents = [
        make_agent(agent, derive_seed!(seed, "agent", 0), false),
        make_agent(agent, derive_seed!(seed, "agent", 1), false),
    ];
    let mut n = 0;
    while n < DECISION_LIMIT {
        let Some(p) = st.pending().map(|d| d.hdr.player) else {
            break;
        };
        let a = agents[usize::from(p)]
            .choose(&st, p, cx)
            .map_err(GameError::agent)?;
        apply(&mut st, a, true)?;
        n += 1;
    }
    Ok((st.winner().unwrap_or(-1), st.first_player()))
}

/// The seat checks of W-152: `(seat-0 Wilson interval contains 0.5, |z| of the first-player
/// rate by seat < 2.576)`, with the counts for the messages.
fn seat_checks(results: &[(i8, u8)]) -> (bool, bool, String) {
    let decided: Vec<&(i8, u8)> = results.iter().filter(|(w, _)| *w == 0 || *w == 1).collect();
    let n = decided.len() as u64;
    let seat0 = decided.iter().filter(|(w, _)| *w == 0).count() as u64;
    let (lo, hi) = wilson(seat0, n, Z95).expect("games");
    let first = |s: u8| {
        let g: Vec<_> = decided.iter().filter(|(_, f)| *f == s).collect();
        let wins = g
            .iter()
            .filter(|(w, f)| i16::from(*w) == i16::from(*f))
            .count();
        (wins as f64, g.len() as f64)
    };
    let ((w0, n0), (w1, n1)) = (first(0), first(1));
    let pooled = (w0 + w1) / (n0 + n1);
    let se = (pooled * (1.0 - pooled) * (1.0 / n0 + 1.0 / n1)).sqrt();
    let z = if se > 0.0 {
        (w0 / n0 - w1 / n1) / se
    } else {
        0.0
    };
    let msg = format!(
        "seat 0 won {seat0}/{n} [{lo:.4}, {hi:.4}]; first player won {w0}/{n0} from seat 0 and {w1}/{n1} from seat 1 (z = {z:.3})"
    );
    (lo <= 0.5 && 0.5 <= hi, z.abs() < 2.576, msg)
}

fn mirror_run(agent: &AgentDesc, games: u32, master: u64, handicap: bool) -> Vec<(i8, u8)> {
    let reg = Registry::embedded();
    let decks = gcg_eval::decks::dir_decks(&root().join("examples/decks")).expect("examples");
    run_indexed(
        usize::try_from(games).unwrap_or(0),
        Workers::Count(10),
        &Cancel::new(),
        &mut |_| {},
        |i, cx| {
            let deck = &decks[i % decks.len()].deck;
            mirror_game(
                &reg,
                agent,
                deck,
                derive_seed!(master, "mirror", i),
                handicap,
                cx,
            )
        },
        |i| {
            (
                u32::try_from(i).unwrap_or(0),
                derive_seed!(master, "mirror", i),
            )
        },
    )
    .expect("mirror games")
}

#[test]
fn w152_seats_and_first_player_are_symmetric() {
    let random = parse_agent_spec("random").expect("random");
    let r = mirror_run(&random, 4000, 152, false);
    let (seat_ok, first_ok, msg) = seat_checks(&r);
    println!("W-152 random: {msg}");
    assert!(seat_ok && first_ok, "random mirror games: {msg}");
    let fast = parse_agent_spec("search:fast").expect("search:fast");
    let f = mirror_run(&fast, 400, 1520, false);
    let (seat_ok, first_ok, msg) = seat_checks(&f);
    println!("W-152 search:fast: {msg}");
    assert!(seat_ok && first_ok, "search:fast mirror games: {msg}");
}

#[test]
fn w152_negative_control_two_extra_shields_fail_the_seat_check() {
    let random = parse_agent_spec("random").expect("random");
    let r = mirror_run(&random, 4000, 152, true);
    let (seat_ok, _, msg) = seat_checks(&r);
    println!("W-152 negative control: {msg}");
    assert!(
        !seat_ok,
        "two extra Shields for seat 0 must break seat symmetry: {msg}"
    );
}
