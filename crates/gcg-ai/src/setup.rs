//! Pre-game choices (`ai/setup.py`): who goes first (rule 6-2-1-4) and whether to redraw
//! (rule 6-2-1-6).

use std::sync::Arc;

use gcg_core::pymath::py_sum;
use gcg_core::rng::SplitMix64;
use gcg_data::{CardDef, CardType, DefId};
use gcg_effects::Registry;
use gcg_engine::observe::determinize;
use gcg_engine::view::link_satisfied_unpatched;
use gcg_engine::{Action, ActionKind, GameState, Zone};

use crate::math::FloatMath;
use crate::playout::play_to_end_with;
use crate::{AgentCx, AgentError, Alternative, PolicyKnobs, Scratch};

/// `OPENING_TURNS` (`ai/setup.py:22`).
const OPENING_TURNS: i32 = 4;
/// `KEEP_RATIO` (`ai/setup.py:23`).
const KEEP_RATIO: f64 = 0.9;
/// `START_HAND` (`ai/setup.py:24`).
const START_HAND: usize = 5;
/// `FIRST_PLAYER_TEMPERATURE` (`ai/setup.py:25`).
const FIRST_PLAYER_TEMPERATURE: f64 = 0.5;
/// `MAX_GAME_DECISIONS` (`ai/setup.py:26`).
const MAX_GAME_DECISIONS: u32 = 3000;
/// `SIGNIFICANCE` (`ai/setup.py:27`).
const SIGNIFICANCE: f64 = 2.0;

/// `list.remove(x)`: drops the first element equal to `x`. `CardDef` equality is field
/// equality, and `def_id` (the first field) is unique, so it is the first with that `def_id`.
fn remove_first(cards: &mut Vec<&CardDef>, x: &CardDef) {
    if let Some(i) = cards.iter().position(|c| c.def_id == x.def_id) {
        cards.remove(i);
    }
}

/// `opening_score(hand, first)` (`ai/setup.py:30-60`): how much board a hand develops over the
/// first four turns if played out greedily. Links are checked with the unpatched
/// `link_satisfied` (`ai/setup.py:18` binds the original function). The scores are plain
/// sequential additions (Inv 07 §12).
#[must_use]
pub fn opening_score(hand: &[&CardDef], first: bool, reg: &Registry) -> f64 {
    let mut cards: Vec<&CardDef> = hand.to_vec();
    // sorted(key=(-cost, card_number)): stable, card numbers by code point (Rust `str` order).
    cards.sort_by(|a, b| {
        (-a.cost)
            .cmp(&-b.cost)
            .then_with(|| a.card_number.cmp(&b.card_number))
    });
    let mut unpaired: Vec<&CardDef> = Vec::new();
    let mut ex = i32::from(!first);
    let mut score = 0.0;
    for turn in 1..=OPENING_TURNS {
        let weight = f64::from(OPENING_TURNS + 1 - turn);
        let budget = turn + ex;
        let mut spent = 0;
        let units: Vec<&CardDef> = cards
            .iter()
            .copied()
            .filter(|c| c.card_type == CardType::Unit)
            .collect();
        for c in units {
            if c.level <= turn + ex && c.cost <= budget - spent {
                spent += c.cost;
                remove_first(&mut cards, c);
                unpaired.push(c);
                score += weight * 0.5 * f64::from(c.ap + c.hp);
            }
        }
        let pilots: Vec<&CardDef> = cards
            .iter()
            .copied()
            .filter(|c| c.card_type == CardType::Pilot)
            .collect();
        for c in pilots {
            if !unpaired.is_empty() && c.level <= turn + ex && c.cost <= budget - spent {
                let unit = unpaired
                    .iter()
                    .copied()
                    .find(|u| link_satisfied_unpatched(reg, u, c))
                    .unwrap_or(unpaired[0]);
                remove_first(&mut unpaired, unit);
                spent += c.cost;
                remove_first(&mut cards, c);
                let bonus = if link_satisfied_unpatched(reg, unit, c) {
                    1.0
                } else {
                    0.0
                };
                score += weight * (0.5 * f64::from(c.ap + c.hp) + bonus);
            }
        }
        if spent > turn {
            ex = 0;
        }
    }
    score
}

/// `sorted(Counter(decklist) - hand)` with `Counter.subtract` semantics (`ai/setup.py:73-75`):
/// every def id with a positive remaining count, repeated, ascending.
fn redraw_pool(decklist: &[DefId], hand_ids: &[DefId]) -> Vec<DefId> {
    let mut counts: Vec<(DefId, i64)> = Vec::new();
    for &d in decklist.iter().chain(hand_ids) {
        if !counts.iter().any(|&(x, _)| x == d) {
            counts.push((d, 0));
        }
    }
    for &d in decklist {
        if let Some(slot) = counts.iter_mut().find(|(x, _)| *x == d) {
            slot.1 += 1;
        }
    }
    for &d in hand_ids {
        if let Some(slot) = counts.iter_mut().find(|(x, _)| *x == d) {
            slot.1 -= 1;
        }
    }
    let mut pool: Vec<DefId> = Vec::new();
    for (d, n) in counts {
        for _ in 0..n.max(0) {
            pool.push(d);
        }
    }
    pool.sort();
    pool
}

/// `mulligan_choice(st, player, samples, rng)` (`ai/setup.py:63-83`): redraw when the hand
/// develops clearly less board than a fresh five from the rest of the deck would on average.
/// One pool is shuffled again for every sample (the shuffles compound), and `total` is a plain
/// `+=` sum.
#[must_use]
pub fn mulligan_choice(
    st: &GameState,
    player: u8,
    samples: u32,
    rng: &mut SplitMix64,
) -> (Action, Vec<Alternative>) {
    let db = st.db();
    let reg = st.registry();
    let hand_ids: Vec<DefId> = st
        .zone(player, Zone::Hand)
        .iter()
        .map(|&u| st.card(u).def_id)
        .collect();
    let first = st.first_player() == player;
    let hand: Vec<&CardDef> = hand_ids.iter().map(|&i| db.by_id(i)).collect();
    let current = opening_score(&hand, first, reg);
    let mut pool = redraw_pool(st.decklists()[usize::from(player)], &hand_ids);
    let mut total = 0.0;
    for _ in 0..samples {
        rng.shuffle(&mut pool);
        let five: Vec<&CardDef> = pool.iter().take(START_HAND).map(|&i| db.by_id(i)).collect();
        total += opening_score(&five, first, reg);
    }
    let average = if samples == 0 {
        0.0
    } else {
        total / f64::from(samples)
    };
    let keep = Action::of(ActionKind::Keep);
    let redraw = Action::of(ActionKind::Redraw);
    let stats = vec![
        Alternative {
            action: keep,
            value: current,
            visits: 1,
        },
        Alternative {
            action: redraw,
            value: KEEP_RATIO * average,
            visits: samples,
        },
    ];
    let chosen = if current >= KEEP_RATIO * average {
        keep
    } else {
        redraw
    };
    (chosen, stats)
}

/// One play/draw simulation of the fork-join: `(state clone, choice, seed)` to its score.
type SimulationJob = Arc<
    dyn Fn(usize, (GameState, Action, u64), &mut Scratch) -> Result<f64, AgentError> + Send + Sync,
>;

/// `_result(st, player, choice, seed, knobs)` (`ai/setup.py:114-120`): one policy-vs-policy
/// game from `choice` in a determinisation; the same seed drives the determinisation and the
/// policy. The choice is applied with `check=True`.
fn simulate(
    st: &GameState,
    player: u8,
    choice: Action,
    seed: u64,
    knobs: &PolicyKnobs,
    math: FloatMath,
) -> Result<f64, AgentError> {
    let mut det = determinize(st, player, seed)?;
    gcg_engine::apply(&mut det, choice, true)?;
    let mut rng = SplitMix64::new(seed);
    play_to_end_with(
        &mut det,
        Some(&mut rng),
        FIRST_PLAYER_TEMPERATURE,
        MAX_GAME_DECISIONS,
        knobs,
        math,
    )?;
    Ok(match det.winner() {
        Some(w) if i32::from(w) == i32::from(player) => 1.0,
        None | Some(-1) => 0.5,
        Some(_) => 0.0,
    })
}

/// `first_player_choice(st, player, games, rng, knobs)` (`ai/setup.py:86-111`): `games` seeds
/// drawn up front, all "first" games then all "second" games on the same determinisations
/// (common random numbers); go second only when that is significantly better.
///
/// The `2 × games` simulations are pure functions of `(st, player, choice, seed, knobs)`, so
/// they run as one fork-join (`cx.par`), each job on its own clone of `st`; the results come
/// back in Python's list order and the sums are taken afterwards in that order (PLAN §6.2
/// item 12). An error is reported for the first failing simulation in Python's order.
///
/// # Errors
/// [`AgentError::Interrupted`] when the SIGINT flag is set before a simulation starts; the
/// first simulation error otherwise.
pub fn first_player_choice(
    st: &GameState,
    player: u8,
    games: u32,
    rng: &mut SplitMix64,
    knobs: &PolicyKnobs,
    cx: &mut AgentCx<'_>,
) -> Result<(Action, Vec<Alternative>), AgentError> {
    first_player_choice_with(st, player, games, rng, knobs, cx, FloatMath::Platform)
}

/// [`first_player_choice`] with `math` in the simulations' Gumbel draws and the paired
/// variance (W-137): at [`FloatMath::Platform`] it is [`first_player_choice`], bit for bit.
///
/// # Errors
/// As [`first_player_choice`].
#[allow(
    clippy::too_many_arguments,
    reason = "the frozen first_player_choice inputs plus the floating-point library"
)]
pub fn first_player_choice_with(
    st: &GameState,
    player: u8,
    games: u32,
    rng: &mut SplitMix64,
    knobs: &PolicyKnobs,
    cx: &mut AgentCx<'_>,
    math: FloatMath,
) -> Result<(Action, Vec<Alternative>), AgentError> {
    let first = Action::new(ActionKind::GoFirst, i32::from(player), -1, -1);
    let second = Action::new(ActionKind::GoFirst, 1 - i32::from(player), -1, -1);
    let seeds: Vec<u64> = (0..games).map(|_| rng.next_u64()).collect();
    if cx.cancel.is_cancelled() {
        return Err(AgentError::Interrupted);
    }
    let inputs: Vec<(GameState, Action, u64)> = [first, second]
        .into_iter()
        .flat_map(|choice| seeds.iter().map(move |&s| (choice, s)))
        .map(|(choice, s)| (st.clone(), choice, s))
        .collect();
    let knobs = *knobs;
    let cancel = cx.cancel.clone();
    let job: SimulationJob = Arc::new(move |_, (state, choice, seed), _| {
        if cancel.is_cancelled() {
            return Err(AgentError::Interrupted);
        }
        simulate(&state, player, choice, seed, &knobs, math)
    });
    let results = cx.par.fork_join(inputs, job);
    let mut scores = Vec::with_capacity(results.len());
    for r in results {
        scores.push(r?);
    }
    let n = seeds.len();
    let (as_first, as_second) = scores.split_at(n);
    let games_f = f64::from(games.max(1));
    let stats = vec![
        Alternative {
            action: first,
            value: py_sum(as_first.iter().copied()) / games_f,
            visits: games,
        },
        Alternative {
            action: second,
            value: py_sum(as_second.iter().copied()) / games_f,
            visits: games,
        },
    ];
    let diffs: Vec<f64> = as_first.iter().zip(as_second).map(|(a, b)| a - b).collect();
    if diffs.len() < 2 {
        return Ok((first, stats));
    }
    let len = diffs.len() as f64;
    let mean = py_sum(diffs.iter().copied()) / len;
    let var = py_sum(diffs.iter().map(|&x| math.square(x - mean))) / (len - 1.0);
    let se = math.sqrt(var / len);
    let chosen = if mean < 0.0 && -mean > SIGNIFICANCE * se {
        second
    } else {
        first
    };
    Ok((chosen, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraw_pool_follows_counter_subtract() {
        let d = |n: u16| DefId(n);
        let deck = [d(3), d(1), d(3), d(2), d(3)];
        // A hand card outside the decklist goes negative and is dropped by elements().
        let hand = [d(3), d(9), d(1)];
        assert_eq!(redraw_pool(&deck, &hand), [d(2), d(3), d(3)]);
        assert!(redraw_pool(&[], &hand).is_empty());
    }
}
