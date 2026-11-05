//! `opponent_list = archetype` (W-134, PLAN §12.3 S5, A30): the opponent's unseen cards come
//! from a declared prior of candidate decklists, never from its true decklist.
//!
//! The reference knows both exact decklists (`docs/ASSUMPTIONS.md:62`): `determinize` draws the
//! opponent's hidden cards from its decklist minus the cards seen (`engine/observe.py:39-83`),
//! and the evaluation's Burst density reads the same pools (`ai/evaluation.py:3-6`).
//!
//! **The prior.** The packaged deck lists: the three example decks and the five evaluation
//! decks (`examples/decks/*.txt`, `examples/decks/eval/*.txt`, copied byte for byte into
//! `crates/gcg-search/decks/prior-*.txt` so the code digest covers them), with equal weights.
//!
//! **The posterior.** Each candidate's weight is multiplied by the multivariate hypergeometric
//! likelihood of the opponent's cards the searcher can see (printed identities; the main-deck
//! cards against the candidate's main deck, the Resources against its resource deck; tokens
//! are no decklist cards): `Π_c C(L[c], V[c]) / C(|L|, |V|)`. When no candidate has a positive
//! likelihood, every candidate keeps its prior weight. Each candidate with a positive weight is
//! patched to contain the seen cards (a seen card the list lacks replaces a copy of the
//! candidate's most numerous unseen card, the lowest def id first), so the patch is a no-op for
//! a consistent list. Only the prior and the opponent's visible cards enter, so the posterior
//! is a function of the information set.
//!
//! **Worlds.** World k draws candidate `j` by `resample(weights, u)` with `u` the first
//! `random()` of `SplitMix64(derive_seed(W[k], "archetype"))`, then is determinised (with
//! `exact_pools` exactly) from the information-set state whose opponent decklists are list `j`
//! (`observe::with_decklists`), with the world's own seed `W[k]`. The root's Burst density of
//! the opponent is the posterior mixture of the candidates' densities (the searcher's own is
//! the reference's). A one-list prior equal to the true list gives the reference's worlds and
//! density exactly, so it reproduces `known` mode bit for bit.
//!
//! **Not covered.** The pre-game decisions (`Mechanisms::pregame`: REDRAW reads only the
//! searcher's own decklist; CHOOSE_FIRST simulations, which the forced play/draw protocol of
//! A24 never asks for) keep the reference's information.

use std::collections::BTreeMap;
use std::sync::Arc;

use gcg_ai::{Density, burst_density};
use gcg_core::pymath::first_max_by;
use gcg_core::rng::{SeedPart, SplitMix64, derive_seed};
use gcg_data::{CardDb, CardType, DefId, parse_deck};
use gcg_engine::observe::{is_hidden_from, with_decklists};
use gcg_engine::{EngineError, GameState};

use super::belief::resample;

/// The packaged deck lists of the built-in prior, by name.
const PACKAGED: [(&str, &str); 8] = [
    (
        "blue-white-federation",
        include_str!("../../../decks/prior-blue-white-federation.txt"),
    ),
    (
        "purple-white-tekkadan",
        include_str!("../../../decks/prior-purple-white-tekkadan.txt"),
    ),
    (
        "red-green-zeon",
        include_str!("../../../decks/prior-red-green-zeon.txt"),
    ),
    (
        "blue-green-ramp",
        include_str!("../../../decks/prior-blue-green-ramp.txt"),
    ),
    (
        "blue-red-tempo",
        include_str!("../../../decks/prior-blue-red-tempo.txt"),
    ),
    (
        "green-white-link",
        include_str!("../../../decks/prior-green-white-link.txt"),
    ),
    (
        "purple-blue-swarm",
        include_str!("../../../decks/prior-purple-blue-swarm.txt"),
    ),
    (
        "red-purple-control",
        include_str!("../../../decks/prior-red-purple-control.txt"),
    ),
];

/// One candidate decklist of a prior.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Candidate {
    pub(crate) name: Box<str>,
    pub(crate) main: Box<[DefId]>,
    pub(crate) res: Box<[DefId]>,
    pub(crate) weight: f64,
}

/// A prior over the opponent's decklist: candidate lists with positive weights.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Prior {
    pub(crate) candidates: Vec<Candidate>,
}

impl Prior {
    /// The built-in prior: the packaged deck lists with equal weights, the card numbers
    /// resolved in `db`.
    ///
    /// # Errors
    /// A packaged list that does not parse, or a card number `db` lacks.
    pub(crate) fn packaged(db: &CardDb) -> Result<Self, EngineError> {
        let mut candidates = Vec::with_capacity(PACKAGED.len());
        for (name, text) in PACKAGED {
            let deck = parse_deck(text, name, None)
                .map_err(|e| EngineError::Value(format!("archetype prior {name}: {e}")))?;
            let expand = |cards: &[gcg_data::CardCount]| -> Result<Box<[DefId]>, EngineError> {
                let mut out = Vec::new();
                for c in cards {
                    let def = db.get(&c.card_number).ok_or_else(|| {
                        EngineError::Value(format!(
                            "archetype prior {name}: unknown card {}",
                            c.card_number
                        ))
                    })?;
                    for _ in 0..c.count {
                        out.push(def.def_id);
                    }
                }
                out.sort();
                Ok(out.into_boxed_slice())
            };
            candidates.push(Candidate {
                name: name.into(),
                main: expand(&deck.main)?,
                res: expand(&deck.resources)?,
                weight: 1.0,
            });
        }
        Ok(Self { candidates })
    }

    /// A prior of one list.
    #[cfg(test)]
    pub(crate) fn one(name: &str, main: &[DefId], res: &[DefId]) -> Self {
        let sorted = |v: &[DefId]| {
            let mut v = v.to_vec();
            v.sort();
            v.into_boxed_slice()
        };
        Self {
            candidates: vec![Candidate {
                name: name.into(),
                main: sorted(main),
                res: sorted(res),
                weight: 1.0,
            }],
        }
    }
}

/// The multiset of the cards of `owner` that `observer` can see, by printed identity: the
/// main-deck cards and the Resources apart; tokens are left out.
fn seen(st: &GameState, observer: u8, owner: u8) -> [BTreeMap<DefId, u32>; 2] {
    let db = st.db();
    let mut out = [BTreeMap::new(), BTreeMap::new()];
    for (i, c) in st.cards().iter().enumerate() {
        let Ok(uid) = gcg_engine::Uid::try_from(i) else {
            continue;
        };
        if c.owner != owner || is_hidden_from(st, uid, observer) {
            continue;
        }
        let def = db.by_id(db.base_def_id(c.def_id));
        if def.card_type.is_token() {
            continue;
        }
        let part = usize::from(def.card_type == CardType::Resource);
        *out[part].entry(def.def_id).or_insert(0) += 1;
    }
    out
}

/// `C(n, k)` as a float, in a fixed order of operations.
fn binomial(n: u64, k: u64) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut out = 1.0f64;
    for i in 0..k {
        out = out * u64_f64(n - i) / u64_f64(i + 1);
    }
    out
}

/// A card count as a float (exact for every count in scope).
fn u64_f64(x: u64) -> f64 {
    f64::from(u32::try_from(x).unwrap_or(u32::MAX))
}

fn counts(list: &[DefId]) -> BTreeMap<DefId, u64> {
    let mut m = BTreeMap::new();
    for &d in list {
        *m.entry(d).or_insert(0) += 1;
    }
    m
}

/// The multivariate hypergeometric probability of drawing `seen` from `list`.
fn likelihood(seen: &BTreeMap<DefId, u32>, list: &[DefId]) -> f64 {
    let c = counts(list);
    let k: u64 = seen.values().map(|&x| u64::from(x)).sum();
    let n = u64::try_from(list.len()).unwrap_or(u64::MAX);
    if k > n {
        return 0.0;
    }
    let mut num = 1.0f64;
    for (d, &x) in seen {
        num *= binomial(c.get(d).copied().unwrap_or(0), u64::from(x));
        if num == 0.0 {
            return 0.0;
        }
    }
    num / binomial(n, k)
}

/// `list` patched to contain `seen`: every missing seen copy replaces one copy of the card with
/// the most unseen copies (the lowest def id first); the list keeps its size and is sorted.
fn patched(list: &[DefId], seen: &BTreeMap<DefId, u32>) -> Box<[DefId]> {
    let mut c = counts(list);
    for (&d, &x) in seen {
        let have = c.get(&d).copied().unwrap_or(0);
        for _ in have..u64::from(x) {
            let spare = first_max_by(
                c.iter()
                    .map(|(&e, &n)| {
                        let shown = seen.get(&e).map_or(0, |&s| u64::from(s));
                        (e, n.saturating_sub(shown))
                    })
                    .filter(|&(_, unseen)| unseen > 0),
                |a, b| a.1.cmp(&b.1),
            );
            if let Some((e, _)) = spare
                && let Some(n) = c.get_mut(&e)
            {
                *n -= 1;
            }
            *c.entry(d).or_insert(0) += 1;
        }
    }
    let mut out = Vec::with_capacity(list.len());
    for (&d, &n) in &c {
        for _ in 0..n {
            out.push(d);
        }
    }
    out.into_boxed_slice()
}

/// One candidate of the posterior: its normalised weight and the information-set state whose
/// opponent decklists are the (patched) candidate.
#[derive(Clone)]
pub(crate) struct Entry {
    pub(crate) name: Box<str>,
    pub(crate) weight: f64,
    pub(crate) state: GameState,
}

/// The posterior of one decision (see the module documentation).
#[derive(Clone)]
pub(crate) struct Posterior {
    pub(crate) entries: Vec<Entry>,
    /// The root's Burst density: the reference's for the searcher, the posterior mixture of
    /// the candidates' for the opponent.
    pub(crate) density: Density,
}

impl std::fmt::Debug for Posterior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Posterior")
            .field(
                "entries",
                &self
                    .entries
                    .iter()
                    .map(|e| (&e.name, e.weight))
                    .collect::<Vec<_>>(),
            )
            .field("density", &self.density)
            .finish()
    }
}

impl PartialEq for Posterior {
    fn eq(&self, other: &Self) -> bool {
        self.density.map(f64::to_bits) == other.density.map(f64::to_bits)
            && self.entries.len() == other.entries.len()
            && self.entries.iter().zip(&other.entries).all(|(a, b)| {
                a.name == b.name
                    && a.weight.to_bits() == b.weight.to_bits()
                    && a.state.decklists() == b.state.decklists()
                    && a.state.resource_decklists() == b.state.resource_decklists()
            })
    }
}

impl Posterior {
    /// The posterior of `prior` at the information-set state `st` of `player`.
    ///
    /// # Errors
    /// `with_decklists`' error for a candidate outside the card database.
    pub(crate) fn of(st: &GameState, player: u8, prior: &Prior) -> Result<Self, EngineError> {
        let opponent = 1 - player;
        let [seen_main, seen_res] = seen(st, player, opponent);
        let raw: Vec<f64> = prior
            .candidates
            .iter()
            .map(|c| c.weight * likelihood(&seen_main, &c.main) * likelihood(&seen_res, &c.res))
            .collect();
        let consistent = raw.iter().any(|&w| w > 0.0);
        let weights: Vec<f64> = if consistent {
            raw
        } else {
            prior.candidates.iter().map(|c| c.weight).collect()
        };
        let total: f64 = weights.iter().filter(|&&w| w > 0.0).sum();
        let own = st.decklists()[usize::from(player)];
        let own_res = st.resource_decklists()[usize::from(player)];
        let mut entries = Vec::new();
        for (c, &w) in prior.candidates.iter().zip(&weights) {
            if w <= 0.0 {
                continue;
            }
            let main = patched(&c.main, &seen_main);
            let res = patched(&c.res, &seen_res);
            let (lists, res_lists) = if player == 0 {
                ([own.into(), main], [own_res.into(), res])
            } else {
                ([main, own.into()], [res, own_res.into()])
            };
            entries.push(Entry {
                name: c.name.clone(),
                weight: w / total,
                state: with_decklists(st, lists, res_lists)?,
            });
        }
        let mut density = burst_density(st, player);
        density[usize::from(opponent)] = entries.iter().fold(0.0, |acc, e| {
            acc + e.weight * burst_density(&e.state, player)[usize::from(opponent)]
        });
        Ok(Self { entries, density })
    }

    /// The candidate world `k` draws (an index into `entries`).
    pub(crate) fn draw(&self, world_seed: u64) -> usize {
        if self.entries.len() == 1 {
            return 0;
        }
        let u = SplitMix64::new(derive_seed(world_seed, &[SeedPart::Str("archetype")])).random();
        let weights: Vec<f64> = self.entries.iter().map(|e| e.weight).collect();
        resample(&weights, u)
    }
}

/// The prior every search in `opponent_list = archetype` mode uses unless a test set another.
///
/// # Errors
/// As [`Prior::packaged`].
pub(crate) fn default_prior(st: &GameState) -> Result<Arc<Prior>, EngineError> {
    Prior::packaged(st.db()).map(Arc::new)
}

/// The cards of the opponent of `player` hidden from `player`, as a sorted multiset of printed
/// identities (for tests and diagnostics).
#[cfg(test)]
pub(crate) fn hidden_of(st: &GameState, player: u8) -> Vec<DefId> {
    use gcg_engine::observe::HIDDEN_ZONES;
    let db = st.db();
    let opponent = 1 - player;
    let mut out: Vec<DefId> = (0..2u8)
        .flat_map(|side| HIDDEN_ZONES.iter().map(move |&z| (side, z)))
        .flat_map(|(side, z)| st.zone(side, z).iter().copied())
        .filter(|&u| st.card(u).owner == opponent && is_hidden_from(st, u, player))
        .map(|u| db.base_def_id(st.card(u).def_id))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binomials_and_likelihoods() {
        assert_eq!(binomial(4, 2), 6.0);
        assert_eq!(binomial(50, 0), 1.0);
        assert_eq!(binomial(3, 4), 0.0);
        let (a, b) = (DefId(10), DefId(11));
        let list = [a, a, a, b];
        let one = |d: DefId, n: u32| BTreeMap::from([(d, n)]);
        // P(one specific card of 4 drawn) = 3/4 for a, 1/4 for b.
        assert_eq!(likelihood(&one(a, 1), &list), 0.75);
        assert_eq!(likelihood(&one(b, 1), &list), 0.25);
        assert_eq!(likelihood(&one(b, 2), &list), 0.0);
        assert_eq!(likelihood(&BTreeMap::new(), &list), 1.0);
    }

    #[test]
    fn patches_add_the_seen_cards_and_keep_the_size() {
        let (a, b, c) = (DefId(1), DefId(2), DefId(3));
        let list = [a, a, b, b, b];
        let seen = BTreeMap::from([(c, 2u32), (a, 1)]);
        let p = patched(&list, &seen);
        assert_eq!(p.len(), 5);
        // Two b's (the most unseen copies) make room for the two c's.
        assert_eq!(&*p, &[a, a, b, c, c]);
        // A consistent list is unchanged.
        assert_eq!(&*patched(&list, &BTreeMap::from([(b, 3u32)])), &list);
    }

    #[test]
    fn the_packaged_prior_is_the_eight_panel_decks() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (name, text) in PACKAGED {
            let example = root.join(format!("examples/decks/{name}.txt"));
            let eval = root.join(format!("examples/decks/eval/{name}.txt"));
            let file = std::fs::read_to_string(&example)
                .or_else(|_| std::fs::read_to_string(&eval))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(text, file, "{name}: the copy is the packaged list");
        }
        let prior = Prior::packaged(CardDb::embedded()).expect("the prior parses");
        assert_eq!(prior.candidates.len(), 8);
        for c in &prior.candidates {
            assert_eq!((c.main.len(), c.res.len()), (50, 10), "{}", c.name);
        }
    }
}
