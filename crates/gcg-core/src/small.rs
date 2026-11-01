//! Fixed-capacity inline arrays with an overflow error (docs/port/PLAN.md §2.1, §4.3
//! "Capacities").
//!
//! Inline records of the engine (`EventPayload`, `DecisionHdr.context`, `OnceKey`) never drop an
//! element: an insert beyond the capacity is a [`CapacityError`], which the engine reports as
//! `EngineError::Capacity`. Capacities come from the Phase 2 container census (§8.6).

use std::hash::{Hash, Hasher};

/// At most `N` elements stored inline (`N <= 255`); `Copy` when `T` is, so records that hold
/// one stay `Copy` and `clone_from` stays a `memcpy` (PLAN §4.1).
#[derive(Clone, Copy, Debug)]
pub struct InlineVec<T, const N: usize> {
    len: u8,
    items: [T; N],
}

/// An insert into a full [`InlineVec`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CapacityError {
    pub capacity: usize,
}

impl<T: Copy + Default, const N: usize> InlineVec<T, N> {
    /// An empty array. Unused slots hold `T::default()` and are never observable.
    #[must_use]
    pub fn new() -> Self {
        const { assert!(N <= u8::MAX as usize, "InlineVec capacity must fit in u8") };
        Self {
            len: 0,
            items: [T::default(); N],
        }
    }

    /// Appends `item`, or fails without changing `self` when the array is full.
    ///
    /// # Errors
    /// [`CapacityError`] when `self.len() == N`.
    pub fn try_push(&mut self, item: T) -> Result<(), CapacityError> {
        let i = usize::from(self.len);
        if i == N {
            return Err(CapacityError { capacity: N });
        }
        self.items[i] = item;
        self.len += 1;
        Ok(())
    }

    /// Removes every element.
    pub fn clear(&mut self) {
        self.len = 0;
        self.items = [T::default(); N];
    }
}

impl<T, const N: usize> InlineVec<T, N> {
    pub const CAPACITY: usize = N;

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The elements in insertion order.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.items[..usize::from(self.len)]
    }
}

impl<T: Copy + Default, const N: usize> Default for InlineVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Equality of the stored elements only.
impl<T: PartialEq, const N: usize> PartialEq for InlineVec<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: Eq, const N: usize> Eq for InlineVec<T, N> {}

impl<T: Hash, const N: usize> Hash for InlineVec<T, N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_until_full_then_error_without_change() {
        let mut v: InlineVec<i32, 3> = InlineVec::new();
        assert!(v.is_empty());
        for x in [7, 8, 9] {
            v.try_push(x).unwrap();
        }
        assert_eq!(v.try_push(10), Err(CapacityError { capacity: 3 }));
        assert_eq!(v.as_slice(), &[7, 8, 9]);
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn equality_ignores_unused_slots() {
        let mut a: InlineVec<u8, 4> = InlineVec::new();
        let mut b: InlineVec<u8, 4> = InlineVec::new();
        a.try_push(1).unwrap();
        a.try_push(2).unwrap();
        a.clear();
        a.try_push(5).unwrap();
        b.try_push(5).unwrap();
        assert_eq!(a, b);
    }
}
