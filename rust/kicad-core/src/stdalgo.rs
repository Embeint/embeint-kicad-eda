// SPDX-License-Identifier: GPL-3.0-or-later
//! The GNU libstdc++ (`bits/stl_algo.h` and `bits/stl_heap.h`, read from GCC 15)
//! implementations of `std::sort`, `std::nth_element` and `std::partition` for random
//! access ranges, as the pinned oracle compiles them.
//!
//! The C++ standard fixes what these algorithms establish but not the arrangement of
//! the elements they leave unordered, nor the order of equivalent elements.
//! `KI_MST::FilterKruskal` reorders its caller's edges with them and selects edges in
//! the resulting order, so the port follows libstdc++ step by step: introsort with a
//! 16-element insertion-sort threshold, median-of-three pivots, the heap fallback at a
//! depth limit of `2 * floor(log2 n)`, introselect, and the bidirectional partition.
//!
//! libstdc++ moves elements through temporaries ("holes"). Each hole movement here is
//! the equivalent sequence of swaps: the moved value always sits at the hole, so every
//! comparison sees the same pair of values and the final arrangement is identical.
//!
//! The "unguarded" loops of the reference rely on the comparator being a strict weak
//! ordering; with any other comparator they run outside the range (undefined). Here
//! such a run stops with [`StdAlgoError::InvalidOrdering`] instead.

/// Failures of the algorithms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdAlgoError<E> {
    /// An unguarded loop left the range: the comparator is not a strict weak ordering.
    InvalidOrdering,
    /// The predicate failed.
    Predicate(E),
}

/// `_S_threshold`.
const THRESHOLD: usize = 16;

/// `std::__lg( n )`: `floor( log2( n ) )` for `n > 0`.
fn lg(n: usize) -> usize {
    (usize::BITS - 1 - n.leading_zeros()) as usize
}

/// A comparator over positions of one slice.
struct Ranks<'a, T, L> {
    v: &'a mut [T],
    less: &'a mut L,
}

impl<T, L: FnMut(&T, &T) -> bool> Ranks<'_, T, L> {
    /// `__comp( a, b )` on the elements at positions `a` and `b`.
    fn less(
        &mut self,
        a: usize,
        b: usize,
    ) -> Result<bool, StdAlgoError<core::convert::Infallible>> {
        match (self.v.get(a), self.v.get(b)) {
            (Some(x), Some(y)) => Ok((self.less)(x, y)),
            _ => Err(StdAlgoError::InvalidOrdering),
        }
    }

    /// `std::iter_swap`.
    fn swap(&mut self, a: usize, b: usize) -> Result<(), StdAlgoError<core::convert::Infallible>> {
        if a >= self.v.len() || b >= self.v.len() {
            return Err(StdAlgoError::InvalidOrdering);
        }
        self.v.swap(a, b);
        Ok(())
    }

    /// `std::__move_median_to_first( result, a, b, c )`.
    fn move_median_to_first(&mut self, result: usize, a: usize, b: usize, c: usize) -> Res {
        if self.less(a, b)? {
            if self.less(b, c)? {
                self.swap(result, b)
            } else if self.less(a, c)? {
                self.swap(result, c)
            } else {
                self.swap(result, a)
            }
        } else if self.less(a, c)? {
            self.swap(result, a)
        } else if self.less(b, c)? {
            self.swap(result, c)
        } else {
            self.swap(result, b)
        }
    }

    /// `std::__unguarded_partition( first, last, pivot )`.
    fn unguarded_partition(
        &mut self,
        mut first: usize,
        mut last: usize,
        pivot: usize,
    ) -> Result<usize, StdAlgoError<core::convert::Infallible>> {
        loop {
            while self.less(first, pivot)? {
                first += 1;
            }
            last = last.checked_sub(1).ok_or(StdAlgoError::InvalidOrdering)?;
            while self.less(pivot, last)? {
                last = last.checked_sub(1).ok_or(StdAlgoError::InvalidOrdering)?;
            }
            if first >= last {
                return Ok(first);
            }
            self.swap(first, last)?;
            first += 1;
        }
    }

    /// `std::__unguarded_partition_pivot( first, last )`.
    fn unguarded_partition_pivot(
        &mut self,
        first: usize,
        last: usize,
    ) -> Result<usize, StdAlgoError<core::convert::Infallible>> {
        let mid = first + (last - first) / 2;
        self.move_median_to_first(first, first + 1, mid, last - 1)?;
        self.unguarded_partition(first + 1, last, first)
    }

    /// `std::__unguarded_linear_insert( last )`: the value at `last` moves down while it
    /// is less than its predecessor.
    fn unguarded_linear_insert(&mut self, mut last: usize) -> Res {
        let mut next = last.checked_sub(1).ok_or(StdAlgoError::InvalidOrdering)?;
        while self.less(last, next)? {
            self.swap(last, next)?;
            last = next;
            next = next.checked_sub(1).ok_or(StdAlgoError::InvalidOrdering)?;
        }
        Ok(())
    }

    /// `std::__insertion_sort( first, last )`.
    fn insertion_sort(&mut self, first: usize, last: usize) -> Res {
        if first == last {
            return Ok(());
        }
        for i in first + 1..last {
            if self.less(i, first)? {
                // val = move( *i ); move_backward( first, i, i + 1 ); *first = move( val )
                self.v
                    .get_mut(first..=i)
                    .ok_or(StdAlgoError::InvalidOrdering)?
                    .rotate_right(1);
            } else {
                self.unguarded_linear_insert(i)?;
            }
        }
        Ok(())
    }

    /// `std::__unguarded_insertion_sort( first, last )`.
    fn unguarded_insertion_sort(&mut self, first: usize, last: usize) -> Res {
        for i in first..last {
            self.unguarded_linear_insert(i)?;
        }
        Ok(())
    }

    /// `std::__final_insertion_sort( first, last )`.
    fn final_insertion_sort(&mut self, first: usize, last: usize) -> Res {
        if last - first > THRESHOLD {
            self.insertion_sort(first, first + THRESHOLD)?;
            self.unguarded_insertion_sort(first + THRESHOLD, last)
        } else {
            self.insertion_sort(first, last)
        }
    }

    /// `std::__push_heap( first, hole, top, value )` with the value at `first + hole`.
    fn push_heap(&mut self, first: usize, mut hole: usize, top: usize) -> Res {
        // ( hole - 1 ) / 2 in signed arithmetic: 0 for hole 0.
        let parent_of = |hole: usize| hole.saturating_sub(1) / 2;
        let mut parent = parent_of(hole);
        while hole > top && self.less(first + parent, first + hole)? {
            self.swap(first + hole, first + parent)?;
            hole = parent;
            parent = parent_of(hole);
        }
        Ok(())
    }

    /// `std::__adjust_heap( first, hole, len, value )` with the value at `first + hole`.
    fn adjust_heap(&mut self, first: usize, mut hole: usize, len: usize) -> Res {
        let top = hole;
        let mut second = hole;
        while len > 0 && second < (len - 1) / 2 {
            second = 2 * (second + 1);
            if self.less(first + second, first + (second - 1))? {
                second -= 1;
            }
            self.swap(first + hole, first + second)?;
            hole = second;
        }
        if len & 1 == 0 && len >= 2 && second == (len - 2) / 2 {
            second = 2 * (second + 1);
            self.swap(first + hole, first + (second - 1))?;
            hole = second - 1;
        }
        self.push_heap(first, hole, top)
    }

    /// `std::__make_heap( first, last )`.
    fn make_heap(&mut self, first: usize, last: usize) -> Res {
        if last - first < 2 {
            return Ok(());
        }
        let len = last - first;
        let mut parent = (len - 2) / 2;
        loop {
            self.adjust_heap(first, parent, len)?;
            if parent == 0 {
                return Ok(());
            }
            parent -= 1;
        }
    }

    /// `std::__pop_heap( first, last, result )`: the value at `result` replaces the top,
    /// which moves to `result`.
    fn pop_heap(&mut self, first: usize, last: usize, result: usize) -> Res {
        self.swap(result, first)?;
        self.adjust_heap(first, 0, last - first)
    }

    /// `std::__heap_select( first, middle, last )`.
    fn heap_select(&mut self, first: usize, middle: usize, last: usize) -> Res {
        self.make_heap(first, middle)?;
        for i in middle..last {
            if self.less(i, first)? {
                self.pop_heap(first, middle, i)?;
            }
        }
        Ok(())
    }

    /// `std::__sort_heap( first, last )`.
    fn sort_heap(&mut self, first: usize, mut last: usize) -> Res {
        while last - first > 1 {
            last -= 1;
            self.pop_heap(first, last, last)?;
        }
        Ok(())
    }

    /// `std::__introsort_loop( first, last, depth_limit )`. The recursion depth is
    /// bounded by the depth limit, at most `2 * 63`.
    fn introsort_loop(&mut self, first: usize, mut last: usize, mut depth_limit: usize) -> Res {
        while last - first > THRESHOLD {
            if depth_limit == 0 {
                // std::__partial_sort( first, last, last )
                self.heap_select(first, last, last)?;
                return self.sort_heap(first, last);
            }
            depth_limit -= 1;
            let cut = self.unguarded_partition_pivot(first, last)?;
            if cut < first || cut > last {
                return Err(StdAlgoError::InvalidOrdering);
            }
            self.introsort_loop(cut, last, depth_limit)?;
            last = cut;
        }
        Ok(())
    }

    /// `std::__introselect( first, nth, last, depth_limit )`.
    fn introselect(
        &mut self,
        mut first: usize,
        nth: usize,
        mut last: usize,
        mut depth_limit: usize,
    ) -> Res {
        while last - first > 3 {
            if depth_limit == 0 {
                self.heap_select(first, nth + 1, last)?;
                // Place the nth largest element in its final position.
                return self.swap(first, nth);
            }
            depth_limit -= 1;
            let cut = self.unguarded_partition_pivot(first, last)?;
            if cut < first || cut > last {
                return Err(StdAlgoError::InvalidOrdering);
            }
            if cut <= nth {
                first = cut;
            } else {
                last = cut;
            }
        }
        self.insertion_sort(first, last)
    }
}

type Res = Result<(), StdAlgoError<core::convert::Infallible>>;

fn widen<E>(error: StdAlgoError<core::convert::Infallible>) -> StdAlgoError<E> {
    match error {
        StdAlgoError::InvalidOrdering => StdAlgoError::InvalidOrdering,
        StdAlgoError::Predicate(never) => match never {},
    }
}

/// `std::sort( v.begin(), v.end(), less )`.
///
/// # Errors
/// [`StdAlgoError::InvalidOrdering`] if `less` is not a strict weak ordering and the
/// reference would leave the range.
pub fn sort<T, L, E>(v: &mut [T], less: &mut L) -> Result<(), StdAlgoError<E>>
where
    L: FnMut(&T, &T) -> bool,
{
    let n = v.len();
    if n == 0 {
        return Ok(());
    }
    let mut ranks = Ranks { v, less };
    ranks
        .introsort_loop(0, n, lg(n) * 2)
        .and_then(|()| ranks.final_insertion_sort(0, n))
        .map_err(widen)
}

/// `std::nth_element( v.begin(), v.begin() + nth, v.end(), less )`.
///
/// # Errors
/// As [`sort`].
pub fn nth_element<T, L, E>(v: &mut [T], nth: usize, less: &mut L) -> Result<(), StdAlgoError<E>>
where
    L: FnMut(&T, &T) -> bool,
{
    let n = v.len();
    if n == 0 || nth >= n {
        return Ok(());
    }
    let mut ranks = Ranks { v, less };
    ranks.introselect(0, nth, n, lg(n) * 2).map_err(widen)
}

/// `std::partition( v.begin(), v.end(), pred )` (the bidirectional algorithm libstdc++
/// uses for random access iterators). Returns the length of the `true` prefix.
///
/// # Errors
/// [`StdAlgoError::Predicate`] when the predicate fails; the range is then partly
/// rearranged, as the reference would be after an exception.
pub fn partition<T, P, E>(v: &mut [T], mut pred: P) -> Result<usize, StdAlgoError<E>>
where
    P: FnMut(&T) -> Result<bool, E>,
{
    let mut test = |v: &[T], i: usize| -> Result<bool, StdAlgoError<E>> {
        let item = v.get(i).ok_or(StdAlgoError::InvalidOrdering)?;
        pred(item).map_err(StdAlgoError::Predicate)
    };
    let mut first = 0;
    let mut last = v.len();
    loop {
        loop {
            if first == last {
                return Ok(first);
            } else if test(v, first)? {
                first += 1;
            } else {
                break;
            }
        }
        last -= 1;
        loop {
            if first == last {
                return Ok(first);
            } else if !test(v, last)? {
                last -= 1;
            } else {
                break;
            }
        }
        v.swap(first, last);
        first += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed-seed generator for the tests.
    fn values(seed: u64, n: usize, range: u64) -> Vec<u64> {
        let mut state = seed;
        (0..n)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (state >> 33) % range
            })
            .collect()
    }

    #[test]
    fn sort_sorts_and_nth_element_partitions() -> Result<(), StdAlgoError<()>> {
        for (n, range) in [
            (0, 5),
            (1, 5),
            (17, 3),
            (100, 1000),
            (3000, 7),
            (5000, 1 << 40),
        ] {
            let mut v = values(n as u64 + 1, n, range);
            let mut expected = v.clone();
            expected.sort_unstable();
            sort::<_, _, ()>(&mut v, &mut |a: &u64, b: &u64| a < b)?;
            assert_eq!(v, expected);

            let mut w = values(n as u64 + 7, n, range);
            let mid = n / 2;
            nth_element::<_, _, ()>(&mut w, mid, &mut |a: &u64, b: &u64| a < b)?;
            if let Some(&pivot) = w.get(mid) {
                assert!(w[..mid].iter().all(|x| *x <= pivot));
                assert!(w[mid..].iter().all(|x| *x >= pivot));
            }
        }
        Ok(())
    }

    #[test]
    fn heap_fallback_sorts() -> Result<(), StdAlgoError<()>> {
        // A depth limit of 0 from the start goes straight to the heap sort.
        let mut v = values(3, 200, 50);
        let mut expected = v.clone();
        expected.sort_unstable();
        let mut less = |a: &u64, b: &u64| a < b;
        let mut ranks = Ranks {
            v: &mut v,
            less: &mut less,
        };
        ranks.introsort_loop(0, 200, 0).map_err(widen)?;
        assert_eq!(v, expected);
        Ok(())
    }

    #[test]
    fn partition_matches_bidirectional_order() -> Result<(), StdAlgoError<()>> {
        let mut v = [1, 2, 3, 4, 5, 6];
        let split = partition(&mut v, |x: &i32| Ok::<_, ()>(x % 2 == 0))?;
        assert_eq!((split, v), (3, [6, 2, 4, 3, 5, 1]));
        Ok(())
    }

    /// McIlroy's adversary ("A killer adversary for quicksort", 1999): values are fixed
    /// lazily so that each pivot is as bad as possible. Returns the input that makes
    /// `sort` (or `nth_element` at `nth`) compare exactly as it did here.
    fn killer(n: usize, nth: Option<usize>) -> Result<Vec<u64>, StdAlgoError<()>> {
        let gas = n as u64;
        let mut val = vec![gas; n];
        let mut solid = 0u64;
        let mut candidate = 0usize;
        let mut items: Vec<usize> = (0..n).collect();
        {
            let mut less = |x: &usize, y: &usize| {
                if val[*x] == gas && val[*y] == gas {
                    let frozen = if *x == candidate { *x } else { *y };
                    val[frozen] = solid;
                    solid += 1;
                }
                if val[*x] == gas {
                    candidate = *x;
                } else if val[*y] == gas {
                    candidate = *y;
                }
                val[*x] < val[*y]
            };
            match nth {
                None => sort::<_, _, ()>(&mut items, &mut less)?,
                Some(nth) => nth_element::<_, _, ()>(&mut items, nth, &mut less)?,
            }
        }
        for v in &mut val {
            if *v == gas {
                *v = solid;
                solid += 1;
            }
        }
        Ok(val)
    }

    /// The adversary's inputs reach both depth-limit fallbacks: running the same
    /// input without a depth limit leaves a different arrangement.
    #[test]
    fn adversary_reaches_the_heap_fallbacks() -> Result<(), StdAlgoError<()>> {
        let mut less = |a: &u64, b: &u64| a < b;

        let input = killer(1000, None)?;
        let (mut limited, mut unlimited) = (input.clone(), input.clone());
        Ranks {
            v: &mut limited,
            less: &mut less,
        }
        .introsort_loop(0, 1000, lg(1000) * 2)
        .map_err(widen)?;
        Ranks {
            v: &mut unlimited,
            less: &mut less,
        }
        .introsort_loop(0, 1000, usize::MAX)
        .map_err(widen)?;
        assert_ne!(limited, unlimited);

        let input = killer(2048, Some(1024))?;
        let (mut limited, mut unlimited) = (input.clone(), input.clone());
        Ranks {
            v: &mut limited,
            less: &mut less,
        }
        .introselect(0, 1024, 2048, lg(2048) * 2)
        .map_err(widen)?;
        Ranks {
            v: &mut unlimited,
            less: &mut less,
        }
        .introselect(0, 1024, 2048, usize::MAX)
        .map_err(widen)?;
        assert_ne!(limited, unlimited);
        Ok(())
    }

    #[test]
    fn inconsistent_order_is_refused() {
        let mut v = values(9, 500, 100);
        let result = sort::<_, _, ()>(&mut v, &mut |_: &u64, _: &u64| true);
        assert_eq!(result, Err(StdAlgoError::InvalidOrdering));
    }
}
