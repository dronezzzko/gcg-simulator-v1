//! Fixed-capacity inline arrays with an overflow error (docs/dev/PLAN.md §2.1, §4.3
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

    /// The elements in insertion order, writable.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.items[..usize::from(self.len)]
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

/// A list of `Copy` elements kept inline up to `N` and on the heap beyond, in insertion order
/// either way: a drop-in for a short-lived `Vec<T>` that is usually small, so the common case
/// never allocates (docs/dev/PLAN.md §11.4 step 1). It reads as a slice.
#[derive(Clone)]
pub struct SmallBuf<T, const N: usize> {
    repr: Repr<T, N>,
}

#[derive(Clone)]
enum Repr<T, const N: usize> {
    Inline { len: u8, items: [T; N] },
    Heap(Vec<T>),
}

impl<T: Copy + Default, const N: usize> SmallBuf<T, N> {
    /// An empty list.
    #[must_use]
    pub fn new() -> Self {
        const { assert!(N <= u8::MAX as usize, "SmallBuf capacity must fit in u8") };
        Self {
            repr: Repr::Inline {
                len: 0,
                items: [T::default(); N],
            },
        }
    }

    /// Appends `item`, moving to the heap when the inline part is full.
    pub fn push(&mut self, item: T) {
        match &mut self.repr {
            Repr::Inline { len, items } => {
                let n = usize::from(*len);
                if n < N {
                    items[n] = item;
                    *len += 1;
                } else {
                    let mut v = Vec::with_capacity(2 * N + 1);
                    v.extend_from_slice(&items[..]);
                    v.push(item);
                    self.repr = Repr::Heap(v);
                }
            }
            Repr::Heap(v) => v.push(item),
        }
    }

    /// Appends every element of `items` in order.
    pub fn extend_from_slice(&mut self, items: &[T]) {
        for &x in items {
            self.push(x);
        }
    }

    /// Removes every element.
    pub fn clear(&mut self) {
        match &mut self.repr {
            Repr::Inline { len, .. } => *len = 0,
            Repr::Heap(v) => v.clear(),
        }
    }

    /// Keeps the first `n` elements.
    pub fn truncate(&mut self, n: usize) {
        match &mut self.repr {
            Repr::Inline { len, .. } => {
                if n < usize::from(*len) {
                    // n < len <= N <= u8::MAX
                    *len = n as u8;
                }
            }
            Repr::Heap(v) => v.truncate(n),
        }
    }

    /// `Vec::retain`: keeps the elements for which `f` holds, in order.
    pub fn retain(&mut self, mut f: impl FnMut(&T) -> bool) {
        let mut kept = 0;
        let s = self.as_mut_slice();
        for i in 0..s.len() {
            if f(&s[i]) {
                s[kept] = s[i];
                kept += 1;
            }
        }
        self.truncate(kept);
    }

    /// `Vec::insert`.
    ///
    /// # Panics
    /// If `index > len`.
    pub fn insert(&mut self, index: usize, item: T) {
        let n = self.len();
        assert!(index <= n, "insertion index {index} is out of bounds ({n})");
        self.push(item);
        self.as_mut_slice()[index..].rotate_right(1);
    }

    /// `Vec::remove`.
    ///
    /// # Panics
    /// If `index >= len`.
    pub fn remove(&mut self, index: usize) -> T {
        let n = self.len();
        assert!(index < n, "removal index {index} is out of bounds ({n})");
        let s = self.as_mut_slice();
        let x = s[index];
        s[index..].rotate_left(1);
        self.truncate(n - 1);
        x
    }

    /// `Vec::pop`.
    pub fn pop(&mut self) -> Option<T> {
        let n = self.len();
        if n == 0 {
            return None;
        }
        let x = self.as_slice()[n - 1];
        self.truncate(n - 1);
        Some(x)
    }

    /// `Vec::dedup`: drops consecutive repeats.
    pub fn dedup(&mut self)
    where
        T: PartialEq,
    {
        let s = self.as_mut_slice();
        if s.is_empty() {
            return;
        }
        let mut kept = 1;
        for i in 1..s.len() {
            if s[i] != s[kept - 1] {
                s[kept] = s[i];
                kept += 1;
            }
        }
        self.truncate(kept);
    }
}

impl<T, const N: usize> SmallBuf<T, N> {
    /// The elements in order.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        match &self.repr {
            Repr::Inline { len, items } => &items[..usize::from(*len)],
            Repr::Heap(v) => v,
        }
    }

    /// The elements in order, writable.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        match &mut self.repr {
            Repr::Inline { len, items } => &mut items[..usize::from(*len)],
            Repr::Heap(v) => v,
        }
    }
}

impl<T: Copy + Default, const N: usize> Default for SmallBuf<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> std::ops::Deref for SmallBuf<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T, const N: usize> std::ops::DerefMut for SmallBuf<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<T: Copy + Default, const N: usize> FromIterator<T> for SmallBuf<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut out = Self::new();
        out.extend(iter);
        out
    }
}

impl<T: Copy + Default, const N: usize> Extend<T> for SmallBuf<T, N> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for x in iter {
            self.push(x);
        }
    }
}

impl<'a, T: Copy + Default + 'a, const N: usize> Extend<&'a T> for SmallBuf<T, N> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        for &x in iter {
            self.push(x);
        }
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a SmallBuf<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

/// The by-value iterator of a [`SmallBuf`] (elements are `Copy`).
pub struct SmallBufIter<T, const N: usize> {
    buf: SmallBuf<T, N>,
    next: usize,
}

impl<T: Copy, const N: usize> Iterator for SmallBufIter<T, N> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        let x = self.buf.as_slice().get(self.next).copied();
        self.next += usize::from(x.is_some());
        x
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.buf.as_slice().len() - self.next;
        (n, Some(n))
    }
}

impl<T: Copy, const N: usize> ExactSizeIterator for SmallBufIter<T, N> {}

impl<T: Copy, const N: usize> IntoIterator for SmallBuf<T, N> {
    type Item = T;
    type IntoIter = SmallBufIter<T, N>;
    fn into_iter(self) -> Self::IntoIter {
        SmallBufIter { buf: self, next: 0 }
    }
}

impl<T: std::fmt::Debug, const N: usize> std::fmt::Debug for SmallBuf<T, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<T: PartialEq, const N: usize> PartialEq for SmallBuf<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: Eq, const N: usize> Eq for SmallBuf<T, N> {}

impl<T: PartialEq, const N: usize, const M: usize> PartialEq<[T; M]> for SmallBuf<T, N> {
    fn eq(&self, other: &[T; M]) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: PartialEq, const N: usize> PartialEq<[T]> for SmallBuf<T, N> {
    fn eq(&self, other: &[T]) -> bool {
        self.as_slice() == other
    }
}

impl<T: PartialEq, const N: usize> PartialEq<&[T]> for SmallBuf<T, N> {
    fn eq(&self, other: &&[T]) -> bool {
        self.as_slice() == *other
    }
}

impl<T: PartialEq, const N: usize> PartialEq<Vec<T>> for SmallBuf<T, N> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: Hash, const N: usize> Hash for SmallBuf<T, N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_buffers_behave_like_vecs_across_the_spill() {
        let mut b: SmallBuf<u16, 3> = SmallBuf::new();
        let mut v: Vec<u16> = Vec::new();
        for x in [5, 1, 4, 1, 5, 9, 2, 6] {
            b.push(x);
            v.push(x);
            assert_eq!(b, v);
        }
        b.retain(|&x| x != 1);
        v.retain(|&x| x != 1);
        assert_eq!(b, v);
        b.insert(0, 7);
        v.insert(0, 7);
        b.insert(b.len(), 8);
        v.insert(v.len(), 8);
        assert_eq!(b.remove(2), v.remove(2));
        assert_eq!(b.pop(), v.pop());
        assert_eq!(b, v);
        b.sort();
        v.sort();
        assert_eq!(b, v);
        b.dedup();
        v.dedup();
        assert_eq!(b, v);
        b.truncate(2);
        v.truncate(2);
        assert_eq!(b, v);
        assert_eq!(b.clone().into_iter().collect::<Vec<_>>(), v);
        b.clear();
        assert!(b.is_empty() && b.pop().is_none());
        let mut small: SmallBuf<u16, 4> = [3, 3, 2].into_iter().collect();
        small.insert(1, 9);
        assert_eq!(small, [3, 9, 3, 2]);
        assert_eq!(small.remove(0), 3);
        small.dedup();
        assert_eq!(small, [9, 3, 2]);
        assert_eq!(format!("{small:?}"), "[9, 3, 2]");
    }

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
