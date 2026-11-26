// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/include/core/filter_kruskal.h`: `KI_MST::FilterKruskal`, a
//! minimum spanning forest by Filter-Kruskal (Osipov, Sanders and Singler, 2009).
//!
//! The function reorders its caller's edges, and both that arrangement and the order
//! of the emitted edges follow the reference's `std::sort`, `std::nth_element` and
//! `std::partition`, ported in [`crate::stdalgo`].

use crate::stdalgo::{self, StdAlgoError};
use crate::union_find::{KiUnionFind, UnionFindError};

/// `KRUSKAL_THRESHOLD`: sort and scan a range of this size or smaller instead of
/// splitting it again.
pub const KRUSKAL_THRESHOLD: usize = 1024;

/// `FilterKruskal` failures. Each is undefined behaviour in the reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KruskalError {
    /// An endpoint is not below `aForest.Size()`.
    UnionFind(UnionFindError),
    /// `aLess` is not a strict weak ordering and a libstdc++ loop would leave the
    /// range.
    InvalidOrdering,
}

impl From<UnionFindError> for KruskalError {
    fn from(error: UnionFindError) -> Self {
        Self::UnionFind(error)
    }
}

impl From<StdAlgoError<UnionFindError>> for KruskalError {
    fn from(error: StdAlgoError<UnionFindError>) -> Self {
        match error {
            StdAlgoError::InvalidOrdering => Self::InvalidOrdering,
            StdAlgoError::Predicate(error) => Self::UnionFind(error),
        }
    }
}

/// The state the reference's lambdas capture by reference.
struct Run<'a, L, P, M> {
    forest: &'a mut KiUnionFind,
    less: L,
    endpoints: P,
    emit: M,
    emitted: u64,
}

impl<L, P, M> Run<'_, L, P, M> {
    /// The `kruskal` lambda: sort the range and run plain Kruskal over it.
    fn kruskal<E>(&mut self, range: &mut [E]) -> Result<(), KruskalError>
    where
        L: FnMut(&E, &E) -> bool,
        P: FnMut(&E) -> (u64, u64),
        M: FnMut(&E),
    {
        stdalgo::sort::<_, _, UnionFindError>(range, &mut self.less)?;

        for edge in range.iter() {
            let (u, v) = (self.endpoints)(edge);

            if self.forest.unite(u, v)? {
                (self.emit)(edge);
                self.emitted = self.emitted.wrapping_add(1);
            }
        }

        Ok(())
    }

    /// The `filter` lambda: move the edges whose endpoints are not yet joined to the
    /// front and return their count. An edge whose endpoints are already joined closes
    /// a cycle, so no minimum spanning forest can hold it.
    fn filter<E>(&mut self, range: &mut [E]) -> Result<usize, KruskalError>
    where
        P: FnMut(&E) -> (u64, u64),
    {
        let Self {
            forest, endpoints, ..
        } = self;
        Ok(stdalgo::partition(range, |edge: &E| {
            let (u, v) = endpoints(edge);
            forest.connected(u, v).map(|joined| !joined)
        })?)
    }

    /// The `recurse` lambda. Each level splits at the median, so the depth is at most
    /// `log2( n / KRUSKAL_THRESHOLD ) + 1`.
    fn recurse<E>(&mut self, range: &mut [E]) -> Result<(), KruskalError>
    where
        L: FnMut(&E, &E) -> bool,
        P: FnMut(&E) -> (u64, u64),
        M: FnMut(&E),
    {
        // Nothing later can be selected once everything is joined.
        if range.is_empty() || self.forest.component_count() == 1 {
            return Ok(());
        }

        if range.len() <= KRUSKAL_THRESHOLD {
            return self.kruskal(range);
        }

        // nth_element places the median with the lighter edges ahead of it, so both
        // halves shrink. It is also deterministic, unlike a random pivot.
        let mid = range.len() / 2;
        stdalgo::nth_element::<_, _, UnionFindError>(range, mid, &mut self.less)?;

        let (lighter, heavier) = range.split_at_mut(mid);
        self.recurse(lighter)?;

        if self.forest.component_count() == 1 {
            return Ok(());
        }

        let kept = self.filter(heavier)?;
        let candidates = heavier
            .get_mut(..kept)
            .ok_or(KruskalError::InvalidOrdering)?;
        self.recurse(candidates)
    }
}

/// `FilterKruskal( aEdges, aForest, aLess, aEndpoints, aEmit )`: build a minimum
/// spanning forest over `edges`, which it reorders.
///
/// `forest` may already hold unions (known-connected pairs); the edges they imply are
/// discarded. `less` must be a strict weak ordering (a total order on edges, not only
/// their weights, for a platform-independent tree); `endpoints` maps an edge to its two
/// vertex indices (`size_t` in the reference); `emit` receives each selected edge in
/// order of increasing weight. Returns the number of edges emitted.
///
/// # Errors
/// [`KruskalError`] where the reference is undefined; edges and `forest` are then
/// partly processed.
pub fn filter_kruskal<E, L, P, M>(
    edges: &mut [E],
    forest: &mut KiUnionFind,
    less: L,
    endpoints: P,
    emit: M,
) -> Result<u64, KruskalError>
where
    L: FnMut(&E, &E) -> bool,
    P: FnMut(&E) -> (u64, u64),
    M: FnMut(&E),
{
    let mut run = Run {
        forest,
        less,
        endpoints,
        emit,
        emitted: 0,
    };
    run.recurse(edges)?;
    Ok(run.emitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Edge {
        u: u32,
        v: u32,
        w: u64,
        id: u32,
    }

    fn edges(seed: u64, n: usize, vertices: u32) -> Vec<Edge> {
        let mut state = seed;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            state >> 33
        };
        (0..n as u32)
            .map(|id| Edge {
                u: (next() % u64::from(vertices)) as u32,
                v: (next() % u64::from(vertices)) as u32,
                w: next() % 1000,
                id,
            })
            .collect()
    }

    /// The forest's total weight equals plain Kruskal's on a dense random graph.
    #[test]
    fn matches_plain_kruskal_weight() -> Result<(), KruskalError> {
        let vertices = 300;
        let mut input = edges(5, 6000, vertices);
        let mut plain = input.clone();
        let less = |a: &Edge, b: &Edge| (a.w, a.id) < (b.w, b.id);

        let mut forest = KiUnionFind::new(u64::from(vertices))?;
        let mut chosen = Vec::new();
        let count = filter_kruskal(
            &mut input,
            &mut forest,
            less,
            |e: &Edge| (u64::from(e.u), u64::from(e.v)),
            |e: &Edge| chosen.push(*e),
        )?;

        plain.sort_by_key(|e| (e.w, e.id));
        let mut reference = KiUnionFind::new(u64::from(vertices))?;
        let mut expected = Vec::new();
        for e in &plain {
            if reference.unite(u64::from(e.u), u64::from(e.v))? {
                expected.push(*e);
            }
        }

        assert_eq!(count as usize, chosen.len());
        assert_eq!(chosen, expected);
        assert_eq!(forest.component_count(), reference.component_count());
        Ok(())
    }

    #[test]
    fn single_component_returns_before_sorting() -> Result<(), KruskalError> {
        let mut input = edges(1, 10, 1);
        let before = input.clone();
        let mut forest = KiUnionFind::new(1)?;
        let count = filter_kruskal(
            &mut input,
            &mut forest,
            |a: &Edge, b: &Edge| a.w < b.w,
            |e: &Edge| (u64::from(e.u), u64::from(e.v)),
            |_: &Edge| {},
        )?;
        assert_eq!((count, input), (0, before));
        Ok(())
    }

    #[test]
    fn endpoint_out_of_range_is_refused() -> Result<(), KruskalError> {
        let mut input = edges(1, 10, 9);
        let mut forest = KiUnionFind::new(4)?;
        let result = filter_kruskal(
            &mut input,
            &mut forest,
            |a: &Edge, b: &Edge| a.w < b.w,
            |e: &Edge| (u64::from(e.u), u64::from(e.v)),
            |_: &Edge| {},
        );
        assert_eq!(
            result,
            Err(KruskalError::UnionFind(UnionFindError::IndexOutOfRange))
        );
        Ok(())
    }
}
