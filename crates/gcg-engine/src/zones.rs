//! `zones[p][z]` (`engine/state.py:479`) as one arena (docs/port/PLAN.md §4.3): list
//! `i = 12 * p + z` is `items[offs[i]..offs[i + 1]]`, so `clone_from` copies one `Vec` and one
//! offset array. Every edit is one `memmove` inside `items` plus an offset update; the order of
//! every list is Python's (`list.insert(0, u)`, `list.append`, `list.remove` of the first
//! occurrence, `core.py:38-44`, `:102-106`).

use crate::{EngineError, N_ZONES, Uid, Zone, ZoneLists};

/// Number of zone lists (`2 * N_ZONES`).
const LISTS: usize = 2 * N_ZONES;

/// What [`EngineError::Overflow`] names when the arena outgrows its `u16` offsets.
const ARENA: &str = "zone arena (u16 offsets)";

impl ZoneLists {
    /// `[[[] for _ in range(N_ZONES)] for _ in range(2)]` (`engine/state.py:479`).
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            items: Vec::new(),
            offs: [0; LISTS + 1],
        }
    }

    /// The index of `zones[p][z]` in `offs`.
    fn list(p: u8, z: Zone) -> usize {
        debug_assert!(p < 2, "player index {p} out of range");
        N_ZONES * usize::from(p) + z.index()
    }

    fn bounds(&self, i: usize) -> (usize, usize) {
        (usize::from(self.offs[i]), usize::from(self.offs[i + 1]))
    }

    /// `st.zones[p][z]` (`engine/state.py:514-515`).
    #[must_use]
    pub(crate) fn get(&self, p: u8, z: Zone) -> &[Uid] {
        let (a, b) = self.bounds(Self::list(p, z));
        &self.items[a..b]
    }

    /// The list for an in-place permutation (`st.rng.shuffle(deck)`, `core.py:134-139`); the
    /// length cannot change through it.
    #[allow(
        dead_code,
        reason = "called by layers L3-L5 (docs/port/contracts.md); they land after L1"
    )]
    pub(crate) fn get_mut(&mut self, p: u8, z: Zone) -> &mut [Uid] {
        let (a, b) = self.bounds(Self::list(p, z));
        &mut self.items[a..b]
    }

    /// Adds `n` to the end offsets of list `i` and of every later list.
    fn grow_from(&mut self, i: usize, n: usize) -> Result<(), EngineError> {
        let total = self.items.len() + n;
        if u16::try_from(total).is_err() {
            return Err(EngineError::Overflow { what: ARENA });
        }
        let n = u16::try_from(n).map_err(|_| EngineError::Overflow { what: ARENA })?;
        for off in &mut self.offs[i + 1..] {
            *off += n;
        }
        Ok(())
    }

    /// Subtracts `n` from the end offsets of list `i` and of every later list.
    fn shrink_from(&mut self, i: usize, n: u16) {
        for off in &mut self.offs[i + 1..] {
            *off -= n;
        }
    }

    /// `lst.insert(index, uid)` (`core.py:104`, index 0 = top); an index past the end appends,
    /// as in Python.
    ///
    /// # Errors
    /// [`EngineError::Overflow`] past `u16::MAX` uids in all lists together.
    #[allow(
        dead_code,
        reason = "called by layers L3-L5 (docs/port/contracts.md); they land after L1"
    )]
    pub(crate) fn insert(
        &mut self,
        p: u8,
        z: Zone,
        index: usize,
        uid: Uid,
    ) -> Result<(), EngineError> {
        let i = Self::list(p, z);
        let (a, b) = self.bounds(i);
        self.grow_from(i, 1)?;
        self.items.insert(a + index.min(b - a), uid);
        Ok(())
    }

    /// `lst.append(uid)` (`core.py:106`, `:128`).
    ///
    /// # Errors
    /// [`EngineError::Overflow`] past `u16::MAX` uids in all lists together.
    #[allow(
        dead_code,
        reason = "called by layers L3-L5 (docs/port/contracts.md); they land after L1"
    )]
    pub(crate) fn push(&mut self, p: u8, z: Zone, uid: Uid) -> Result<(), EngineError> {
        let i = Self::list(p, z);
        let (_, b) = self.bounds(i);
        self.grow_from(i, 1)?;
        self.items.insert(b, uid);
        Ok(())
    }

    /// `lst.extend(uids)`.
    ///
    /// # Errors
    /// [`EngineError::Overflow`] past `u16::MAX` uids in all lists together.
    pub(crate) fn extend(&mut self, p: u8, z: Zone, uids: &[Uid]) -> Result<(), EngineError> {
        let i = Self::list(p, z);
        let (_, b) = self.bounds(i);
        self.grow_from(i, uids.len())?;
        self.items.splice(b..b, uids.iter().copied());
        Ok(())
    }

    /// `lst[0:0] = uids`: `uids` in their order before the current first element.
    ///
    /// # Errors
    /// [`EngineError::Overflow`] past `u16::MAX` uids in all lists together.
    pub(crate) fn insert_front(&mut self, p: u8, z: Zone, uids: &[Uid]) -> Result<(), EngineError> {
        let i = Self::list(p, z);
        let (a, _) = self.bounds(i);
        self.grow_from(i, uids.len())?;
        self.items.splice(a..a, uids.iter().copied());
        Ok(())
    }

    /// `lst.remove(uid)`: removes the first occurrence and keeps the order of the rest
    /// (`core.py:38-44`).
    ///
    /// # Errors
    /// Python's `ValueError` when `uid` is not in the list.
    pub(crate) fn remove(&mut self, p: u8, z: Zone, uid: Uid) -> Result<(), EngineError> {
        let i = Self::list(p, z);
        let (a, b) = self.bounds(i);
        let Some(k) = self.items[a..b].iter().position(|&u| u == uid) else {
            return Err(EngineError::Value(
                "list.remove(x): x not in list".to_owned(),
            ));
        };
        self.items.remove(a + k);
        self.shrink_from(i, 1);
        Ok(())
    }

    /// `zones[p][z] = list(uids)`: replaces one whole list.
    ///
    /// # Errors
    /// [`EngineError::Overflow`] past `u16::MAX` uids in all lists together.
    pub(crate) fn set(&mut self, p: u8, z: Zone, uids: &[Uid]) -> Result<(), EngineError> {
        let i = Self::list(p, z);
        let (a, b) = self.bounds(i);
        let old = b - a;
        if uids.len() >= old {
            self.grow_from(i, uids.len() - old)?;
        } else {
            let fewer = u16::try_from(old - uids.len())
                .map_err(|_| EngineError::Overflow { what: ARENA })?;
            self.shrink_from(i, fewer);
        }
        self.items.splice(a..b, uids.iter().copied());
        Ok(())
    }

    /// Copies `src` into `self`, reusing `self`'s buffer (PLAN §4.5).
    pub(crate) fn copy_from(&mut self, src: &Self) {
        self.items.clear();
        self.items.extend_from_slice(&src.items);
        self.offs = src.offs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gcg_core::prop;

    fn model_of(z: &ZoneLists) -> Vec<Vec<Uid>> {
        (0..2u8)
            .flat_map(|p| Zone::ALL.into_iter().map(move |zone| (p, zone)))
            .map(|(p, zone)| z.get(p, zone).to_vec())
            .collect()
    }

    #[test]
    fn edits_keep_python_list_order() {
        let mut z = ZoneLists::new();
        z.push(0, Zone::Hand, 5).unwrap();
        z.push(0, Zone::Hand, 7).unwrap();
        z.insert(0, Zone::Deck, 0, 1).unwrap();
        z.insert(0, Zone::Deck, 0, 2).unwrap();
        z.push(1, Zone::Trash, 9).unwrap();
        z.extend(0, Zone::Hand, &[8, 5]).unwrap();
        z.insert_front(0, Zone::Hand, &[3, 4]).unwrap();
        assert_eq!(z.get(0, Zone::Deck), &[2, 1]);
        assert_eq!(z.get(0, Zone::Hand), &[3, 4, 5, 7, 8, 5]);
        z.remove(0, Zone::Hand, 5).unwrap();
        assert_eq!(z.get(0, Zone::Hand), &[3, 4, 7, 8, 5]);
        assert_eq!(z.get(1, Zone::Trash), &[9]);
        assert_eq!(
            z.remove(1, Zone::Hand, 9),
            Err(EngineError::Value(
                "list.remove(x): x not in list".to_owned()
            ))
        );
        z.set(0, Zone::Hand, &[1]).unwrap();
        z.set(0, Zone::Deck, &[6, 6, 6]).unwrap();
        assert_eq!(z.get(0, Zone::Hand), &[1]);
        assert_eq!(z.get(0, Zone::Deck), &[6, 6, 6]);
        assert_eq!(z.get(1, Zone::Trash), &[9]);
        z.get_mut(0, Zone::Deck)[1] = 4;
        assert_eq!(z.items, [6, 4, 6, 1, 9]);
    }

    #[test]
    fn copy_from_reuses_the_buffer() {
        let mut src = ZoneLists::new();
        for u in 0..40 {
            src.push(u8::from(u % 3 == 0), Zone::ALL[usize::from(u % 12)], u)
                .unwrap();
        }
        let mut dst = ZoneLists::new();
        dst.items.reserve(64);
        let ptr = dst.items.as_ptr();
        dst.copy_from(&src);
        assert_eq!(dst, src);
        assert_eq!(dst.items.as_ptr(), ptr);
    }

    #[test]
    fn overflow_past_u16_offsets_is_an_error() {
        let mut z = ZoneLists::new();
        let many: Vec<Uid> = vec![1; usize::from(u16::MAX)];
        z.extend(0, Zone::Deck, &many).unwrap();
        assert_eq!(
            z.push(1, Zone::Hand, 2),
            Err(EngineError::Overflow { what: ARENA })
        );
        assert_eq!(z.get(1, Zone::Hand), &[] as &[Uid]);
        assert_eq!(z.get(0, Zone::Deck).len(), usize::from(u16::MAX));
    }

    /// T-058 (zone part): random edit sequences agree with Python-like `list[list[int]]`.
    #[test]
    fn random_edits_agree_with_a_list_model() {
        prop::check("zone-arena-vs-model", 300, |rng| {
            let mut z = ZoneLists::new();
            let mut model: Vec<Vec<Uid>> = vec![Vec::new(); LISTS];
            for _ in 0..60 {
                let p = u8::from(rng.randrange(2) == 1);
                let zone = Zone::ALL[rng.randrange_usize(N_ZONES)];
                let lst = &mut model[N_ZONES * usize::from(p) + zone.index()];
                let uid = Uid::try_from(rng.randrange(20)).unwrap();
                match rng.randrange(6) {
                    0 => {
                        let at = rng.randrange_usize(lst.len() + 1);
                        lst.insert(at, uid);
                        z.insert(p, zone, at, uid).map_err(|e| format!("{e:?}"))?;
                    }
                    1 => {
                        lst.push(uid);
                        z.push(p, zone, uid).map_err(|e| format!("{e:?}"))?;
                    }
                    2 => {
                        let expect = lst.iter().position(|&u| u == uid);
                        let got = z.remove(p, zone, uid);
                        match expect {
                            Some(k) => {
                                lst.remove(k);
                                got.map_err(|e| format!("{e:?}"))?;
                            }
                            None if got.is_ok() => return Err("removed an absent uid".into()),
                            None => {}
                        }
                    }
                    3 => {
                        let add = [uid, uid / 2, uid + 1];
                        lst.extend_from_slice(&add);
                        z.extend(p, zone, &add).map_err(|e| format!("{e:?}"))?;
                    }
                    4 => {
                        let add = [uid + 3, uid];
                        lst.splice(0..0, add);
                        z.insert_front(p, zone, &add)
                            .map_err(|e| format!("{e:?}"))?;
                    }
                    _ => {
                        let new: Vec<Uid> = (0..rng.randrange(4))
                            .map(|k| uid + Uid::try_from(k).unwrap())
                            .collect();
                        lst.clone_from(&new);
                        z.set(p, zone, &new).map_err(|e| format!("{e:?}"))?;
                    }
                }
                if model_of(&z) != model {
                    return Err(format!("arena {:?} != model {model:?}", model_of(&z)));
                }
            }
            Ok(())
        });
    }
}
