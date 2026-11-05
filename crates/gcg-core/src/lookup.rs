//! A lookup-only hash map (docs/dev/PLAN.md §9.9).
//!
//! `clippy.toml` bans `std::collections::{HashMap, HashSet}` and `hashbrown::{HashMap, HashSet}`
//! so that no iteration order can reach behaviour or output. [`LookupMap`] is the one sanctioned
//! wrapper: a fixed `foldhash` state (no per-process randomness) and no iteration methods.

use std::borrow::Borrow;
use std::fmt;
use std::hash::Hash;

use foldhash::fast::FixedState;

#[expect(
    clippy::disallowed_types,
    reason = "LookupMap is the one sanctioned wrapper of hashbrown (PLAN §9.9)"
)]
type Inner<K, V> = hashbrown::HashMap<K, V, FixedState>;

/// Get, insert and membership only. There is deliberately no `iter`, `keys`, `values`,
/// `drain` or `Debug` listing of entries.
#[derive(Clone)]
pub struct LookupMap<K, V> {
    map: Inner<K, V>,
}

impl<K: Hash + Eq, V> LookupMap<K, V> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: Inner::with_hasher(FixedState::default()),
        }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: Inner::with_capacity_and_hasher(capacity, FixedState::default()),
        }
    }

    /// Inserts `value`, returning the previous value of `key` (Python `d[key] = value`).
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.map.insert(key, value)
    }

    #[must_use]
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.get(key)
    }

    #[must_use]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.get_mut(key)
    }

    #[must_use]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.contains_key(key)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl<K: Hash + Eq, V> Default for LookupMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// Shows the size only: listing entries would expose hash order.
impl<K, V> fmt::Debug for LookupMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LookupMap")
            .field("len", &self.map.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_and_overwrite() {
        let mut m: LookupMap<Box<str>, u16> = LookupMap::new();
        assert_eq!(m.insert("GD01-001".into(), 149), None);
        assert_eq!(m.insert("GD01-001".into(), 150), Some(149));
        assert_eq!(m.get("GD01-001"), Some(&150));
        assert!(m.contains_key("GD01-001"));
        assert!(!m.contains_key("GD01-002"));
        assert_eq!(m.len(), 1);
        assert_eq!(format!("{m:?}"), "LookupMap { len: 1, .. }");
    }
}
