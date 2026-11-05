//! `difflib.get_close_matches(word, possibilities, n=1, cutoff=0.6)` of CPython 3.15.1
//! (`Lib/difflib.py:40-46`, `:121-305`, `:306-491`, `:598-715`), over code points. argparse
//! uses it for the "maybe you meant" suggestion of an invalid choice
//! (`argparse.py:2814-2841`). `SequenceMatcher(None, a, b, autojunk=True)` with `b = word`
//! and `a` each possibility in turn, so the "popular element" pruning applies to the word
//! once it has 200 or more code points. Checked against vectors exported by
//! `port/oracle/export_cli.py`.

use gcg_core::lookup::LookupMap;

/// `_calculate_ratio` (`difflib.py:40-43`).
fn ratio_of(matches: usize, length: usize) -> f64 {
    if length > 0 {
        2.0 * matches as f64 / length as f64
    } else {
        1.0
    }
}

/// `SequenceMatcher` with `isjunk=None`, `autojunk=True` and a fixed `b`.
struct Matcher<'b> {
    b: &'b [u32],
    /// `b2j` (`__chain_b`, `difflib.py:267-304`): the indices of each element of `b`, in
    /// increasing order; empty for "popular" elements.
    b2j: LookupMap<u32, Vec<usize>>,
    /// `fullbcount` (`quick_ratio`, `difflib.py:633-636`).
    bcount: LookupMap<u32, usize>,
}

impl<'b> Matcher<'b> {
    /// `set_seq2(b)` and `__chain_b` (`difflib.py:223-304`).
    fn new(b: &'b [u32]) -> Self {
        let mut b2j: LookupMap<u32, Vec<usize>> = LookupMap::new();
        let mut bcount: LookupMap<u32, usize> = LookupMap::new();
        for (i, &elt) in b.iter().enumerate() {
            match b2j.get_mut(&elt) {
                Some(indices) => indices.push(i),
                None => {
                    b2j.insert(elt, vec![i]);
                }
            }
            match bcount.get_mut(&elt) {
                Some(count) => *count += 1,
                None => {
                    bcount.insert(elt, 1);
                }
            }
        }
        // Popular elements (autojunk, n >= 200) leave b2j; there is no junk (isjunk=None).
        let n = b.len();
        if n >= 200 {
            let ntest = n / 100 + 1;
            for &elt in b {
                if bcount.get(&elt).is_some_and(|&count| count > ntest) {
                    b2j.insert(elt, Vec::new());
                }
            }
        }
        Self { b, b2j, bcount }
    }

    /// `find_longest_match(alo, ahi, blo, bhi)` without junk (`difflib.py:306-420`):
    /// the longest block, earliest in `a`, then earliest in `b`, extended over popular
    /// elements on both sides.
    fn find_longest_match(
        &self,
        a: &[u32],
        alo: usize,
        ahi: usize,
        blo: usize,
        bhi: usize,
    ) -> (usize, usize, usize) {
        let b = self.b;
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0_usize);
        // j2len[j] = length of the longest match ending with a[i-1] and b[j]; dense rows
        // with a list of the entries set in each, so that a row starts empty.
        let mut j2len = vec![0_usize; b.len() + 1];
        let mut set_prev: Vec<usize> = Vec::new();
        let mut newj2len = vec![0_usize; b.len() + 1];
        let mut set_new: Vec<usize> = Vec::new();
        for (i, elt) in a.iter().enumerate().take(ahi).skip(alo) {
            if let Some(indices) = self.b2j.get(elt) {
                for &j in indices {
                    if j < blo {
                        continue;
                    }
                    if j >= bhi {
                        break;
                    }
                    let k = if j > 0 { j2len[j - 1] } else { 0 } + 1;
                    newj2len[j] = k;
                    set_new.push(j);
                    if k > bestsize {
                        besti = i + 1 - k;
                        bestj = j + 1 - k;
                        bestsize = k;
                    }
                }
            }
            for &j in &set_prev {
                j2len[j] = 0;
            }
            std::mem::swap(&mut j2len, &mut newj2len);
            std::mem::swap(&mut set_prev, &mut set_new);
            set_new.clear();
        }
        // No junk: extend over equal (popular) elements on each end.
        while besti > alo && bestj > blo && a[besti - 1] == b[bestj - 1] {
            besti -= 1;
            bestj -= 1;
            bestsize += 1;
        }
        while besti + bestsize < ahi
            && bestj + bestsize < bhi
            && a[besti + bestsize] == b[bestj + bestsize]
        {
            bestsize += 1;
        }
        (besti, bestj, bestsize)
    }

    /// `get_matching_blocks()` (`difflib.py:422-491`) summed: the number of matched elements.
    fn matches(&self, a: &[u32]) -> usize {
        let (la, lb) = (a.len(), self.b.len());
        let mut queue = vec![(0, la, 0, lb)];
        let mut blocks: Vec<(usize, usize, usize)> = Vec::new();
        while let Some((alo, ahi, blo, bhi)) = queue.pop() {
            let (i, j, k) = self.find_longest_match(a, alo, ahi, blo, bhi);
            if k > 0 {
                blocks.push((i, j, k));
                if alo < i && blo < j {
                    queue.push((alo, i, blo, j));
                }
                if i + k < ahi && j + k < bhi {
                    queue.push((i + k, ahi, j + k, bhi));
                }
            }
        }
        // Collapsing adjacent blocks does not change the sum.
        blocks.iter().map(|&(_, _, k)| k).sum()
    }

    /// `ratio()` (`difflib.py:598-621`).
    fn ratio(&self, a: &[u32]) -> f64 {
        ratio_of(self.matches(a), a.len() + self.b.len())
    }

    /// `quick_ratio()` (`difflib.py:623-650`).
    fn quick_ratio(&self, a: &[u32]) -> f64 {
        let mut avail: LookupMap<u32, i64> = LookupMap::new();
        let mut matches = 0;
        for elt in a {
            let numb = match avail.get(elt) {
                Some(&n) => n,
                None => self
                    .bcount
                    .get(elt)
                    .map_or(0, |&c| i64::try_from(c).unwrap_or(i64::MAX)),
            };
            avail.insert(*elt, numb - 1);
            if numb > 0 {
                matches += 1;
            }
        }
        ratio_of(matches, a.len() + self.b.len())
    }

    /// `real_quick_ratio()` (`difflib.py:652-662`).
    fn real_quick_ratio(&self, a: &[u32]) -> f64 {
        let (la, lb) = (a.len(), self.b.len());
        ratio_of(la.min(lb), la + lb)
    }
}

/// `get_close_matches(word, possibilities, 1, 0.6)` (`difflib.py:667-715`): the
/// possibility with the largest `(ratio, possibility)` among those whose ratio is at least
/// 0.6 (`heapq.nlargest(1, ...)` is `max`), if any.
#[must_use]
pub fn close_match<'p>(word: &[u32], possibilities: &[&'p [u32]]) -> Option<&'p [u32]> {
    const CUTOFF: f64 = 0.6;
    let matcher = Matcher::new(word);
    let mut best: Option<(f64, &'p [u32])> = None;
    for &x in possibilities {
        if matcher.real_quick_ratio(x) < CUTOFF || matcher.quick_ratio(x) < CUTOFF {
            continue;
        }
        let ratio = matcher.ratio(x);
        if ratio >= CUTOFF {
            let better = match best {
                None => true,
                Some((r, bx)) => ratio > r || (ratio == r && x > bx),
            };
            if better {
                best = Some((ratio, x));
            }
        }
    }
    best.map(|(_, x)| x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cps(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }

    fn close(word: &str, choices: &[&str]) -> Option<String> {
        let owned: Vec<Vec<u32>> = choices.iter().map(|c| cps(c)).collect();
        let refs: Vec<&[u32]> = owned.iter().map(Vec::as_slice).collect();
        close_match(&cps(word), &refs)
            .map(|m| m.iter().filter_map(|&c| char::from_u32(c)).collect())
    }

    #[test]
    fn docstring_examples() {
        // difflib.py:685-693 (n=1 keeps the best only).
        assert_eq!(
            close("appel", &["ape", "apple", "peach", "puppy"]).as_deref(),
            Some("apple")
        );
        assert_eq!(close("bo5", &["bo3", "bo1"]).as_deref(), Some("bo3"));
        assert_eq!(close("BO3", &["bo3", "bo1"]), None);
    }
}
