//! State invariants of the robustness suite (`engine/invariants.py`; docs/port/contracts.md,
//! gcg-engine `invariants`; Inv 06b §17): card conservation (every non-token card in exactly
//! one location list that matches its `zone`, each player keeping the cards they started
//! with) and legal states (location limits, pairing consistency, damage only where damage can
//! exist, no lethal damage left after rules management, well-formed pending decisions).

use gcg_data::{CardType, DefId};
use gcg_effects::RuleKind;

use crate::view;
use crate::{
    BASE_LIMIT, BATTLE_LIMIT, CardMultiset, CheckError, EX_RESOURCE_LIMIT, EngineError, GameState,
    InvariantViolation, Phase, RESOURCE_LIMIT, TOKEN_ZONES, Zone,
};

/// `LISTED_ZONES = tuple(z for z in Zone if z is not Zone.OUTSIDE)`
/// (`engine/invariants.py:26`), in declaration order.
pub use crate::LISTED_ZONES;

type Key = (u8, DefId);

fn index_error() -> CheckError {
    CheckError::Engine(EngineError::Index("list index out of range".to_owned()))
}

/// The `(owner, base_def_id(def_id))` keys of the non-token cards in uid order: the elements
/// `Counter` counts, in its insertion order (`engine/invariants.py:89-91`, `:114-116`).
fn card_keys(st: &GameState) -> Vec<Key> {
    let db = st.db();
    st.cards()
        .iter()
        .filter(|c| !db.by_id(c.def_id).is_token())
        .map(|c| (c.owner, db.base_def_id(c.def_id)))
        .collect()
}

/// `(key, count)` in first-occurrence order of `keys`.
fn counts_in_order(keys: &[Key]) -> Vec<(Key, u32)> {
    let mut out: Vec<(Key, u32)> = Vec::new();
    for &k in keys {
        if let Some(e) = out.iter_mut().find(|e| e.0 == k) {
            e.1 += 1;
        } else {
            out.push((k, 1));
        }
    }
    out
}

/// `repr(Counter)` (CPython `Counter.__repr__`): `Counter()` when empty, else the
/// `most_common()` order (count descending, stable) as a dict repr.
fn counter_repr(mut entries: Vec<(Key, u32)>) -> String {
    if entries.is_empty() {
        return "Counter()".to_owned();
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.1));
    let items: Vec<String> = entries
        .iter()
        .map(|((owner, def), n)| format!("({owner}, {}): {n}", def.0))
        .collect();
    format!("Counter({{{}}})", items.join(", "))
}

/// `a - b` on Counters (CPython `Counter.__sub__`): the positive differences in `a`'s order.
fn counter_sub(a: &[(Key, u32)], b: &[(Key, u32)]) -> Vec<(Key, u32)> {
    a.iter()
        .filter_map(|&(k, n)| {
            let m = b.iter().find(|e| e.0 == k).map_or(0, |e| e.1);
            (n > m).then_some((k, n - m))
        })
        .collect()
}

/// `f"card multiset changed: {now - initial} / {initial - now}"` (`engine/invariants.py:93`).
/// `now` is in insertion (uid) order, as in Python. [`CardMultiset`] is stored sorted, so
/// `initial`'s insertion order is rebuilt as the first uid of `st` with each key (keys no card
/// of `st` has follow in key order): that is Python's order whenever `initial` was taken from an
/// earlier state of the same game, whose uids keep their owner and printed card.
fn multiset_message(st: &GameState, now: &[(Key, u32)], initial: &CardMultiset) -> String {
    let order: Vec<Key> = card_keys(st);
    let mut init: Vec<(Key, u32)> = initial.0.to_vec();
    init.sort_by_key(|(k, _)| order.iter().position(|o| o == k).unwrap_or(usize::MAX));
    format!(
        "card multiset changed: {} / {}",
        counter_repr(counter_sub(now, &init)),
        counter_repr(counter_sub(&init, now))
    )
}

/// `check(st, initial=None)` (`engine/invariants.py:29-109`): every violation in Python's
/// order; the error's `Display` joins the first ten.
///
/// # Errors
/// [`CheckError::Violation`] with the messages, or [`CheckError::Engine`] for an error of the
/// derived view (or a uid outside the card list, Python's `IndexError`).
pub fn check(st: &GameState, initial: Option<&CardMultiset>) -> Result<(), CheckError> {
    let mut errors: Vec<String> = Vec::new();
    let db = st.db();
    let cards = st.cards();
    let mut seen = vec![0u32; cards.len()];
    for p in 0u8..2 {
        for z in LISTED_ZONES {
            for &uid in st.zone(p, z) {
                let i = usize::from(uid);
                let c = cards.get(i).ok_or_else(index_error)?;
                seen[i] += 1;
                if c.zone != z {
                    errors.push(format!(
                        "uid {uid} listed in {} but zone={}",
                        z.py_name(),
                        c.zone.py_name()
                    ));
                }
                if c.owner != p {
                    errors.push(format!(
                        "uid {uid} owned by {} listed under player {p}",
                        c.owner
                    ));
                }
            }
        }
    }
    let pair_of = |u: i32| -> Result<&crate::CardInstance, CheckError> {
        usize::try_from(u)
            .ok()
            .and_then(|i| cards.get(i))
            .ok_or_else(index_error)
    };
    for (uid, c) in cards.iter().enumerate() {
        let cd = db.by_id(c.def_id);
        if c.zone == Zone::Outside {
            if seen[uid] > 0 {
                errors.push(format!("uid {uid} is OUTSIDE but listed"));
            }
            if !cd.is_token() {
                errors.push(format!(
                    "non-token uid {uid} ({}) left the game",
                    cd.card_number
                ));
            }
            continue;
        }
        if seen[uid] != 1 {
            errors.push(format!(
                "uid {uid} ({}) listed {} times",
                cd.card_number, seen[uid]
            ));
        }
        if cd.is_token() && !TOKEN_ZONES.contains(&c.zone) {
            errors.push(format!("token uid {uid} in {}", c.zone.py_name()));
        }
        if c.damage < 0 {
            errors.push(format!("uid {uid} has negative damage"));
        }
        if c.damage != 0 && !matches!(c.zone, Zone::Battle | Zone::Base) {
            errors.push(format!(
                "uid {uid} has damage {} in {}",
                c.damage,
                c.zone.py_name()
            ));
        }
        let me = i32::try_from(uid).map_err(|_| index_error())?;
        if c.zone == Zone::Paired {
            let u = c.pair;
            let consistent = u >= 0 && {
                let unit = pair_of(u)?;
                unit.zone == Zone::Battle && unit.pair == me
            };
            if !consistent {
                errors.push(format!("pilot uid {uid} paired inconsistently with {u}"));
            }
        } else if c.zone == Zone::Battle {
            if c.pair >= 0 {
                let pilot = pair_of(c.pair)?;
                if pilot.zone != Zone::Paired || pilot.pair != me {
                    errors.push(format!("unit uid {uid} has inconsistent pilot {}", c.pair));
                }
            }
            if !cd.card_type.is_unit() {
                errors.push(format!("non-unit {} in battle area", cd.card_number));
            }
        } else if c.pair >= 0 {
            errors.push(format!(
                "uid {uid} in {} still paired with {}",
                c.zone.py_name(),
                c.pair
            ));
        }
        if c.zone == Zone::Base && !cd.card_type.is_base() {
            errors.push(format!("non-base {} in base section", cd.card_number));
        }
        if matches!(c.zone, Zone::ResourceArea | Zone::ResourceDeck) && !cd.card_type.is_resource()
        {
            errors.push(format!(
                "non-resource {} in {}",
                cd.card_number,
                c.zone.py_name()
            ));
        }
    }
    for p in 0u8..2 {
        let battle = st.zone(p, Zone::Battle).len();
        if battle > BATTLE_LIMIT {
            errors.push(format!("player {p} has {battle} Units (limit 6)"));
        }
        let bases = st.zone(p, Zone::Base).len();
        if bases > BASE_LIMIT {
            errors.push(format!("player {p} has {bases} Bases"));
        }
        let area = st.zone(p, Zone::ResourceArea);
        let n_ex = area
            .iter()
            .filter(|&&u| db.by_id(st.card(u).def_id).card_type == CardType::ExResource)
            .count();
        if area.len() > RESOURCE_LIMIT || n_ex > EX_RESOURCE_LIMIT {
            errors.push(format!("player {p} resource area over limit"));
        }
    }
    if let Some(initial) = initial {
        let now = counts_in_order(&card_keys(st));
        let mut sorted_now = now.clone();
        sorted_now.sort_by_key(|e| e.0);
        if *sorted_now != *initial.0 {
            errors.push(multiset_message(st, &now, initial));
        }
    }
    if st.winner().is_none() {
        match st.pending() {
            None => errors.push("no pending decision in an unfinished game".to_owned()),
            Some(d) if d.options.is_empty() => {
                errors.push(format!("decision {} has no options", d.hdr.kind.as_str()));
            }
            Some(_) => {}
        }
        if st.phase() != Phase::Setup && st.frames().is_empty() {
            let dv = view::derived(st).map_err(CheckError::Engine)?;
            for p in 0u8..2 {
                for zz in [Zone::Battle, Zone::Base] {
                    for &uid in st.zone(p, zz) {
                        if st.card(uid).damage >= view::hp_of(st, &dv, uid)
                            && view::rules_of(&dv, uid, RuleKind::CantBeDestroyed)
                                .next()
                                .is_none()
                        {
                            errors.push(format!("uid {uid} survives with lethal damage"));
                        }
                    }
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(CheckError::Violation(InvariantViolation { errors }))
    }
}

/// `initial_multiset(st)` (`engine/invariants.py:112-116`): `Counter((c.owner,
/// db.base_def_id(c.def_id)) for every non-token card)`, sorted by key.
#[must_use]
pub fn initial_multiset(st: &GameState) -> CardMultiset {
    let mut entries = counts_in_order(&card_keys(st));
    entries.sort_by_key(|e| e.0);
    CardMultiset(entries.into_boxed_slice())
}

impl CardMultiset {
    /// The `((owner, base_def_id), count)` entries of the multiset, sorted by key
    /// (`engine/invariants.py:112-116`); Python's `Counter` order is the order of first
    /// appearance over `st.cards`, which a caller rebuilds from the state.
    #[must_use]
    pub fn entries(&self) -> &[((u8, DefId), u32)] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(owner: u8, def: u16) -> Key {
        (owner, DefId(def))
    }

    #[test]
    fn counter_reprs_follow_most_common_with_stable_ties() {
        assert_eq!(counter_repr(Vec::new()), "Counter()");
        let entries = vec![(key(0, 12), 1), (key(1, 5), 3), (key(0, 3), 1)];
        assert_eq!(
            counter_repr(entries),
            "Counter({(1, 5): 3, (0, 12): 1, (0, 3): 1})"
        );
    }

    #[test]
    fn counter_subtraction_keeps_positive_differences_in_the_left_order() {
        let a = vec![(key(0, 9), 2), (key(0, 1), 1), (key(1, 4), 1)];
        let b = vec![(key(0, 1), 1), (key(0, 9), 1), (key(1, 7), 5)];
        assert_eq!(counter_sub(&a, &b), vec![(key(0, 9), 1), (key(1, 4), 1)]);
        assert_eq!(counter_sub(&b, &a), vec![(key(1, 7), 5)]);
    }

    #[test]
    fn counts_keep_first_occurrence_order() {
        let keys = [key(1, 2), key(0, 1), key(1, 2), key(0, 1), key(0, 0)];
        assert_eq!(
            counts_in_order(&keys),
            vec![(key(1, 2), 2), (key(0, 1), 2), (key(0, 0), 1)]
        );
    }
}
