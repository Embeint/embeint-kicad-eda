// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{order, push, reserve, template, uint, BulkEntry, Coord, Error, NodeData};
use kicad_rtree_node::{hilbert_nd, overlap_mask_2d_dyn};

/// Immutable bottom-up packed storage. Rows retain the reference SoA indexing;
/// there is no second dynamic node or allocator implementation.
pub struct PackedRtree<T: NodeData, E: Coord, const N: usize = 2, const M: usize = 16> {
    bounds: [[Vec<E>; 2]; N],
    counts: Vec<i32>,
    data: Vec<Option<T>>,
    offsets: Vec<usize>,
}
impl<T: NodeData, E: Coord, const N: usize, const M: usize> PackedRtree<T, E, N, M> {
    pub fn new() -> Result<Self, Error> {
        template::<N, M>()?;
        Ok(Self {
            bounds: core::array::from_fn(|_| core::array::from_fn(|_| Vec::new())),
            counts: Vec::new(),
            data: Vec::new(),
            offsets: Vec::new(),
        })
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    /// Linux LP64 reference accounting, not the Rust allocation size/capacity.
    /// All reference storage vectors resize from empty to their final lengths.
    pub fn memory_usage(&self) -> Result<u64, Error> {
        let cells = self
            .bounds
            .iter()
            .flat_map(|a| a.iter())
            .try_fold(0u64, |s, v| {
                s.checked_add(v.len() as u64).ok_or(Error::Overflow)
            })?;
        cells
            .checked_mul(E::REFERENCE_SIZE)
            .and_then(|s| s.checked_add((self.counts.len() as u64).checked_mul(4)?))
            .and_then(|s| s.checked_add((self.data.len() as u64).checked_mul(8)?))
            .and_then(|s| s.checked_add((self.offsets.len() as u64).checked_mul(8)?))
            .ok_or(Error::Overflow)
    }
    pub fn counts(&self) -> &[i32] {
        &self.counts
    }
    pub fn level_offsets(&self) -> &[usize] {
        &self.offsets
    }
    pub fn bounds_row(&self, axis: usize, side: usize) -> Result<&[E], Error> {
        self.bounds
            .get(axis)
            .and_then(|a| a.get(side))
            .map(Vec::as_slice)
            .ok_or(Error::Invariant)
    }
    fn bound(&self, d: usize, side: usize, i: usize) -> Result<E, Error> {
        self.bounds_row(d, side)?
            .get(i)
            .copied()
            .ok_or(Error::Invariant)
    }
    fn set(&mut self, d: usize, side: usize, i: usize, v: E) -> Result<(), Error> {
        *self
            .bounds
            .get_mut(d)
            .and_then(|a| a.get_mut(side))
            .and_then(|a| a.get_mut(i))
            .ok_or(Error::Invariant)? = v;
        Ok(())
    }
    fn index(node: usize, slot: usize) -> Result<usize, Error> {
        node.checked_mul(M)
            .and_then(|i| i.checked_add(slot))
            .ok_or(Error::Overflow)
    }
    fn overlaps(
        &self,
        node: usize,
        slot: usize,
        min: &[E; N],
        max: &[E; N],
    ) -> Result<bool, Error> {
        let i = Self::index(node, slot)?;
        for d in 0..N {
            if self.bound(d, 0, i)? > max[d] || self.bound(d, 1, i)? < min[d] {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn mask(&self, node: usize, min: &[E; N], max: &[E; N]) -> Result<u32, Error> {
        let base = Self::index(node, 0)?;
        let row = |d, side| {
            self.bounds_row(d, side)?
                .get(base..base.checked_add(M).ok_or(Error::Overflow)?)
                .ok_or(Error::Invariant)
        };
        let pair = |a: &[E; N]| match a.as_slice() {
            [x, y] => Ok([*x, *y]),
            _ => Err(Error::Invariant),
        };
        Ok(overlap_mask_2d_dyn(
            M,
            row(0, 0)?,
            row(0, 1)?,
            row(1, 0)?,
            row(1, 1)?,
            *self.counts.get(node).ok_or(Error::Invariant)?,
            &pair(min)?,
            &pair(max)?,
        )?)
    }
    pub fn search(
        &self,
        min: [E; N],
        max: [E; N],
        mut visitor: impl FnMut(&Option<T>) -> bool,
    ) -> Result<i32, Error> {
        if self.is_empty() {
            return Ok(0);
        }
        if N == 2 && (M > 31 || M % 4 != 0) {
            return Err(Error::InvalidTemplate);
        }
        let level = self.offsets.len().checked_sub(1).ok_or(Error::Invariant)?;
        // Frames retain either the actual SIMD mask or next scalar slot. Stop
        // propagates globally, unlike the COW header's void searchImpl.
        let mut stack = Vec::new();
        let root = *self.offsets.get(level).ok_or(Error::Invariant)?;
        let initial = if N == 2 {
            self.mask(root, &min, &max)? as usize
        } else {
            0
        };
        push(&mut stack, (level, root, initial))?;
        let mut found = 0i32;
        while let Some((lev, node, remaining)) = stack.pop() {
            let slot = if N == 2 {
                if remaining == 0 {
                    continue;
                }
                let i = remaining.trailing_zeros() as usize;
                push(&mut stack, (lev, node, remaining & (remaining - 1)))?;
                i
            } else {
                let count = *self.counts.get(node).ok_or(Error::Invariant)? as usize;
                if remaining >= count {
                    continue;
                }
                push(&mut stack, (lev, node, remaining + 1))?;
                if !self.overlaps(node, remaining, &min, &max)? {
                    continue;
                }
                remaining
            };
            let relative = node
                .checked_sub(*self.offsets.get(lev).ok_or(Error::Invariant)?)
                .ok_or(Error::Invariant)?;
            let child = Self::index(relative, slot)?;
            if lev == 0 {
                if let Some(data) = self.data.get(child) {
                    found = found.checked_add(1).ok_or(Error::Overflow)?;
                    if !visitor(data) {
                        return Ok(found);
                    }
                }
            } else {
                let child = child
                    .checked_add(*self.offsets.get(lev - 1).ok_or(Error::Invariant)?)
                    .ok_or(Error::Overflow)?;
                let initial = if N == 2 {
                    self.mask(child, &min, &max)? as usize
                } else {
                    0
                };
                push(&mut stack, (lev - 1, child, initial))?;
            }
        }
        Ok(found)
    }
    pub fn begin(&self) -> PackedCursor<'_, T> {
        PackedCursor {
            data: &self.data,
            index: 0,
        }
    }
    pub fn end(&self) -> PackedCursor<'_, T> {
        PackedCursor {
            data: &self.data,
            index: self.data.len(),
        }
    }
    pub fn move_construct(src: &mut Self) -> Result<Self, Error> {
        Ok(core::mem::replace(src, Self::new()?))
    }
    pub fn move_assign(&mut self, src: &mut Self) -> Result<(), Error> {
        *self = Self::move_construct(src)?;
        Ok(())
    }
    pub fn move_assign_self(&mut self) {}
}
/// Pointer iterator semantics represented by a borrowed slice and checked index.
#[derive(Clone)]
pub struct PackedCursor<'a, T> {
    data: &'a [Option<T>],
    index: usize,
}
impl<T: Clone> PackedCursor<'_, T> {
    pub fn current(&self) -> Result<&Option<T>, Error> {
        self.data.get(self.index).ok_or(Error::Invariant)
    }
    pub fn is_end(&self) -> bool {
        self.index == self.data.len()
    }
    pub fn advance(&mut self) -> Result<(), Error> {
        if self.is_end() {
            return Err(Error::Invariant);
        }
        self.index = self.index.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub fn post_advance(&mut self) -> Result<Self, Error> {
        let old = self.clone();
        self.advance()?;
        Ok(old)
    }
}
impl<T> PartialEq for PackedCursor<'_, T> {
    fn eq(&self, o: &Self) -> bool {
        core::ptr::eq(self.data, o.data) && self.index == o.index
    }
}
impl<T> Eq for PackedCursor<'_, T> {}

pub struct Builder<T: NodeData, E: Coord, const N: usize = 2, const M: usize = 16> {
    items: Vec<BulkEntry<T, E, N>>,
    budget: usize,
}
impl<T: NodeData, E: Coord, const N: usize, const M: usize> Builder<T, E, N, M> {
    pub fn new(max_entries: usize) -> Result<Self, Error> {
        template::<N, M>()?;
        Ok(Self {
            items: Vec::new(),
            budget: max_entries,
        })
    }
    pub fn reserve(&mut self, n: usize) -> Result<(), Error> {
        if n > self.budget {
            return Err(Error::ResourceLimit);
        }
        let extra = n.saturating_sub(self.items.len());
        reserve(&mut self.items, extra)
    }
    pub fn add(&mut self, min: [E; N], max: [E; N], data: Option<T>) -> Result<(), Error> {
        if self.items.len() >= self.budget {
            return Err(Error::ResourceLimit);
        }
        push(&mut self.items, BulkEntry { min, max, data })
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn build(&mut self) -> Result<PackedRtree<T, E, N, M>, Error> {
        let mut tree = PackedRtree::new()?;
        let n = self.items.len();
        if n == 0 {
            return Ok(tree);
        }
        let mut lo = [E::MAX; N];
        let mut hi = [E::LOWEST; N];
        // Separate truncating halves are essential for odd negative centers.
        let center = |e: &BulkEntry<T, E, N>, d: usize| e.min[d].widen() / 2 + e.max[d].widen() / 2;
        let mut low = [i64::MAX; N];
        let mut high = [i64::MIN; N];
        for e in &self.items {
            for d in 0..N {
                let c = center(e, d);
                low[d] = low[d].min(c);
                high[d] = high[d].max(c);
            }
        }
        let mut indices = Vec::new();
        reserve(&mut indices, n)?;
        for (i, e) in self.items.iter().enumerate() {
            let mut coords = [0u32; N];
            for d in 0..N {
                let range = high[d] as f64 - low[d] as f64;
                if range > 0.0 {
                    let normalized = (center(e, d) as f64 - low[d] as f64) / range;
                    coords[d] = uint(normalized * u32::MAX as f64)?;
                }
            }
            indices.push((hilbert_nd(32, &coords)?, i));
        }
        order::sort(&mut indices, |a, b| a.0 < b.0)?;
        reserve(&mut tree.data, n)?;
        for (_, i) in &indices {
            tree.data
                .push(self.items.get(*i).ok_or(Error::Invariant)?.data.clone());
        }
        let ceil = |n: usize| n.checked_add(M - 1).map(|n| n / M).ok_or(Error::Overflow);
        let mut levels = Vec::new();
        let mut count = ceil(n)?;
        push(&mut levels, count)?;
        while count > 1 {
            count = ceil(count)?;
            push(&mut levels, count)?;
        }
        let mut total = 0usize;
        for count in &levels {
            push(&mut tree.offsets, total)?;
            total = total.checked_add(*count).ok_or(Error::Overflow)?;
        }
        let cells = total.checked_mul(M).ok_or(Error::Overflow)?;
        for axis in &mut tree.bounds {
            for row in axis {
                reserve(row, cells)?;
                row.resize(cells, E::MAX);
            }
        }
        reserve(&mut tree.counts, total)?;
        tree.counts.resize(total, 0);
        for (i, (_, idx)) in indices.iter().enumerate() {
            let e = self.items.get(*idx).ok_or(Error::Invariant)?;
            for d in 0..N {
                tree.set(d, 0, i, e.min[d])?;
                tree.set(d, 1, i, e.max[d])?;
            }
            *tree.counts.get_mut(i / M).ok_or(Error::Invariant)? = (i % M + 1) as i32;
        }
        for lev in 1..levels.len() {
            let coff = *tree.offsets.get(lev - 1).ok_or(Error::Invariant)?;
            let poff = *tree.offsets.get(lev).ok_or(Error::Invariant)?;
            for ci in 0..*levels.get(lev - 1).ok_or(Error::Invariant)? {
                let child = coff.checked_add(ci).ok_or(Error::Overflow)?;
                let parent = poff.checked_add(ci / M).ok_or(Error::Overflow)?;
                lo.fill(E::MAX);
                hi.fill(E::LOWEST);
                for slot in 0..*tree.counts.get(child).ok_or(Error::Invariant)? as usize {
                    let i = PackedRtree::<T, E, N, M>::index(child, slot)?;
                    for d in 0..N {
                        lo[d] = lo[d].min(tree.bound(d, 0, i)?);
                        hi[d] = hi[d].max(tree.bound(d, 1, i)?);
                    }
                }
                let i = PackedRtree::<T, E, N, M>::index(parent, ci % M)?;
                for d in 0..N {
                    tree.set(d, 0, i, lo[d])?;
                    tree.set(d, 1, i, hi[d])?;
                }
                *tree.counts.get_mut(parent).ok_or(Error::Invariant)? = (ci % M + 1) as i32;
            }
        }
        self.items.clear();
        Ok(tree)
    }
}
