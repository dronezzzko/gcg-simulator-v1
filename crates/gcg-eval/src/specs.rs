//! Game specs: mirrored blocks of four (`ai/selfplay.py:88-110`), the per-pairing seeding of
//! `ai/compare.py:127-143`, and the deal pairing of W-42 (every two games of a block that
//! share a game seed).
//!
//! Seeds follow Python's paths exactly: the game seed is
//! `derive_seed(master, "block", block, "decks", swap_decks)`, the agent seeds
//! `derive_seed(master, "agent", i, seat)` with `i` the index inside the pairing, and pairing `p`
//! of several plays on `derive_seed(seed, "pairing", p)`; a single pairing uses the seed itself.

use std::fmt;

use gcg_core::derive_seed;
use gcg_data::DeckList;

use crate::{AgentDesc, MIRROR_BLOCK, MirroredSpec, SpecError};

/// `"need at least two decks"` and `f"games must be a positive multiple of {multiple}"`
/// (`ai/compare.py:131-135`).
impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewDecks => f.write_str("need at least two decks"),
            Self::GamesNotMultiple { multiple } => {
                write!(f, "games must be a positive multiple of {multiple}")
            }
        }
    }
}

impl std::error::Error for SpecError {}

/// `mirrored_specs(games, master_seed, challenger, opponent, decks)` (`ai/selfplay.py:88-110`):
/// game `i` is variant `i % 4` of block `i / 4`; `swap_decks, challenger_seat =
/// divmod(variant, 2)`, so games `4b` and `4b + 1` share deal `(d0, d1)` with the agents
/// swapped, and `4b + 2`, `4b + 3` share `(d1, d0)`.
#[must_use]
pub fn mirrored_specs(
    games: u32,
    master: u64,
    challenger: &AgentDesc,
    opponent: &AgentDesc,
    decks: [&DeckList; 2],
) -> Vec<MirroredSpec> {
    (0..games)
        .map(|i| {
            let (block, variant) = (i / MIRROR_BLOCK, i % MIRROR_BLOCK);
            let (swap_decks, challenger_seat) = (variant / 2, variant % 2);
            let deck_pair = if swap_decks == 1 {
                [decks[1].clone(), decks[0].clone()]
            } else {
                [decks[0].clone(), decks[1].clone()]
            };
            let agents = if challenger_seat == 0 {
                [challenger.clone(), opponent.clone()]
            } else {
                [opponent.clone(), challenger.clone()]
            };
            MirroredSpec {
                index: i,
                game_seed: derive_seed!(master, "block", block, "decks", swap_decks),
                agent_seeds: [
                    derive_seed!(master, "agent", i, 0),
                    derive_seed!(master, "agent", i, 1),
                ],
                agents,
                decks: deck_pair,
                challenger: u8::from(challenger_seat == 1),
            }
        })
        .collect()
}

/// `itertools.combinations(range(n), 2)`: `(0, 1), (0, 2), ..., (1, 2), ...`.
#[must_use]
pub fn combinations(n: usize) -> Vec<(usize, usize)> {
    (0..n)
        .flat_map(|a| (a + 1..n).map(move |b| (a, b)))
        .collect()
}

/// The master of pairing `p` when there are `pairings` of them (`ai/compare.py:136-140`).
#[must_use]
pub fn pairing_master(seed: u64, p: usize, pairings: usize) -> u64 {
    if pairings == 1 {
        seed
    } else {
        derive_seed!(seed, "pairing", p)
    }
}

/// The games of every pairing: `games / pairings` mirrored games each on its pairing master
/// ([`pairing_master`] with `index_of(p)` and `pairings_total`), re-indexed in order. Python's
/// `comparison_specs` is the case where the pairs are all combinations and `index_of` is the
/// identity; head-to-head selections of a deck set keep each pair's position in the set's
/// combination list, so a pair plays the same games in every selection.
///
/// # Errors
/// [`SpecError`] as `comparison_specs` checks it: no pair, or `games` not a positive multiple
/// of `4 * pairs`.
pub fn pair_specs(
    games: u32,
    seed: u64,
    challenger: &AgentDesc,
    opponent: &AgentDesc,
    pairs: &[(usize, [&DeckList; 2])],
    pairings_total: usize,
) -> Result<Vec<MirroredSpec>, SpecError> {
    if pairs.is_empty() {
        return Err(SpecError::TooFewDecks);
    }
    let multiple = u32::try_from(pairs.len())
        .ok()
        .and_then(|n| n.checked_mul(MIRROR_BLOCK))
        .unwrap_or(u32::MAX);
    if games == 0 || !games.is_multiple_of(multiple) {
        return Err(SpecError::GamesNotMultiple { multiple });
    }
    let per_pair = games / u32::try_from(pairs.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(usize::try_from(games).unwrap_or(0));
    for (p, decks) in pairs {
        let master = pairing_master(seed, *p, pairings_total);
        for mut s in mirrored_specs(per_pair, master, challenger, opponent, *decks) {
            s.index = u32::try_from(out.len()).unwrap_or(u32::MAX);
            out.push(s);
        }
    }
    Ok(out)
}

/// `comparison_specs(games, seed, challenger, opponent, decks)` (`ai/compare.py:127-143`).
///
/// # Errors
/// [`SpecError`] with Python's message.
pub fn comparison_specs(
    games: u32,
    seed: u64,
    challenger: &AgentDesc,
    opponent: &AgentDesc,
    decks: &[DeckList],
) -> Result<Vec<MirroredSpec>, SpecError> {
    let pairs: Vec<(usize, [&DeckList; 2])> = combinations(decks.len())
        .into_iter()
        .enumerate()
        .map(|(p, (a, b))| (p, [&decks[a], &decks[b]]))
        .collect();
    pair_specs(games, seed, challenger, opponent, &pairs, pairs.len())
}

/// The W-42 deals: the index pairs `(i, i + 1)` of every two games that share a game seed and
/// a deck order, with the agents swapped between seats. The first game of a deal has the
/// challenger in seat 0. Two deals per block of four.
///
/// # Panics
/// If `specs` are not mirrored blocks (a deal whose games differ in seed or decks, or do not
/// swap the challenger), which the spec builders above never produce.
#[must_use]
pub fn deals(specs: &[MirroredSpec]) -> Vec<(usize, usize)> {
    specs
        .chunks(2)
        .enumerate()
        .filter(|(_, pair)| pair.len() == 2)
        .map(|(k, pair)| {
            let (a, b) = (&pair[0], &pair[1]);
            assert!(
                a.game_seed == b.game_seed
                    && a.decks == b.decks
                    && a.challenger == 0
                    && b.challenger == 1,
                "games {} and {} are not one deal",
                a.index,
                b.index
            );
            (2 * k, 2 * k + 1)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinations_follow_itertools() {
        assert_eq!(combinations(3), [(0, 1), (0, 2), (1, 2)]);
        assert_eq!(combinations(8).len(), 28);
        assert!(combinations(1).is_empty() && combinations(0).is_empty());
    }

    #[test]
    fn errors_read_as_python() {
        assert_eq!(
            SpecError::TooFewDecks.to_string(),
            "need at least two decks"
        );
        assert_eq!(
            SpecError::GamesNotMultiple { multiple: 12 }.to_string(),
            "games must be a positive multiple of 12"
        );
    }
}
