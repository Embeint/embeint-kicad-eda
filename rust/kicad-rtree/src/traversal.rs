// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{add, mul, order, sub, Coord, DynamicRtree, Error, NodeData, Result};
use kicad_rtree_node::NodeHandle;

/// Lazy cursor for Iterator and SearchIterator. Both compare only end flags in
/// the reference (two different non-end positions compare equal). Current is a
/// copied payload, retained when advance reaches end. Rust's borrow prevents any
/// tree mutation for the cursor's lifetime instead of exposing dangling pointers.
pub struct TreeIterator<'a, T: NodeData, E: Coord, const D: usize, const F: usize> {
    tree: &'a DynamicRtree<T, E, D, F>,
    stack: Vec<(NodeHandle, usize)>,
    query: Option<([E; D], [E; D])>,
    current: Option<T>,
    at_end: bool,
}
impl<'a, T: NodeData, E: Coord, const D: usize, const F: usize> TreeIterator<'a, T, E, D, F> {
    fn new(
        tree: &'a DynamicRtree<T, E, D, F>,
        query: Option<([E; D], [E; D])>,
        end: bool,
    ) -> Result<Self> {
        tree.ready()?;
        let mut it = Self {
            tree,
            stack: Vec::new(),
            query,
            current: None,
            at_end: true,
        };
        if !end {
            if let Some(root) = tree.root.as_ref() {
                if tree.node(root)?.count > 0 {
                    it.stack.try_reserve(1).map_err(|_| Error::Resource)?;
                    it.stack.push((root.clone(), 0));
                    it.advance()?;
                }
            }
        }
        Ok(it)
    }
    pub fn at_end(&self) -> bool {
        self.at_end
    }
    pub fn current(&self) -> Option<&T> {
        self.current.as_ref()
    }
    pub fn advance(&mut self) -> Result<()> {
        while let Some((h, i)) = self.stack.last().cloned() {
            let n = self.tree.node(&h)?;
            if i >= usize::try_from(n.count).map_err(|_| Error::Invariant)? {
                self.stack.pop();
                continue;
            }
            self.stack.last_mut().ok_or(Error::Invariant)?.1 += 1;
            if let Some((min, max)) = self.query.as_ref() {
                if !n.child_overlaps(i, min, max)? {
                    continue;
                }
            }
            if n.is_leaf() {
                self.current = Some(self.tree.data(&h, i)?);
                self.at_end = false;
                return Ok(());
            }
            self.stack.try_reserve(1).map_err(|_| Error::Resource)?;
            self.stack.push((self.tree.child(&h, i)?, 0));
        }
        self.at_end = true;
        Ok(())
    }
}
impl<T: NodeData, E: Coord, const D: usize, const F: usize> PartialEq
    for TreeIterator<'_, T, E, D, F>
{
    fn eq(&self, other: &Self) -> bool {
        self.at_end == other.at_end
    }
}
/// A lazy borrowed SearchRange; constructing it copies bounds, without searching.
pub struct SearchRange<'a, T: NodeData, E: Coord, const D: usize, const F: usize> {
    tree: &'a DynamicRtree<T, E, D, F>,
    min: [E; D],
    max: [E; D],
}
impl<'a, T: NodeData, E: Coord, const D: usize, const F: usize> SearchRange<'a, T, E, D, F> {
    pub fn begin(&self) -> Result<TreeIterator<'a, T, E, D, F>> {
        TreeIterator::new(self.tree, Some((self.min, self.max)), false)
    }
    pub fn end(&self) -> Result<TreeIterator<'a, T, E, D, F>> {
        TreeIterator::new(self.tree, None, true)
    }
    pub fn empty(&self) -> Result<bool> {
        Ok(self.begin()?.at_end())
    }
}
impl<T: NodeData, E: Coord, const D: usize, const F: usize> DynamicRtree<T, E, D, F> {
    pub fn begin(&self) -> Result<TreeIterator<'_, T, E, D, F>> {
        TreeIterator::new(self, None, false)
    }
    pub fn end(&self) -> Result<TreeIterator<'_, T, E, D, F>> {
        TreeIterator::new(self, None, true)
    }
    pub fn overlapping(&self, min: [E; D], max: [E; D]) -> Result<SearchRange<'_, T, E, D, F>> {
        self.ready()?;
        Ok(SearchRange {
            tree: self,
            min,
            max,
        })
    }
    /// Counts the stopping hit too. Any maximum-hit policy belongs in the visitor;
    /// this pinned header has no separate max-hit Search overload.
    pub fn search<V: FnMut(&T) -> bool>(
        &self,
        min: [E; D],
        max: [E; D],
        visitor: &mut V,
    ) -> Result<i32> {
        self.ready()?;
        let mut found = 0;
        if let Some(root) = self.root.as_ref() {
            self.search_impl(root, &min, &max, visitor, &mut found)?;
        }
        Ok(found)
    }
    fn search_impl<V: FnMut(&T) -> bool>(
        &self,
        h: &NodeHandle,
        min: &[E; D],
        max: &[E; D],
        visitor: &mut V,
        found: &mut i32,
    ) -> Result<bool> {
        let n = self.node(h)?;
        let mut mask = n.child_overlap_mask(min, max)?;
        while mask != 0 {
            let i = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if n.is_leaf() {
                *found = found.checked_add(1).ok_or(Error::Overflow)?;
                if !visitor(self.node(h)?.data(i)?.ok_or(Error::Invariant)?) {
                    return Ok(false);
                }
            } else if !self.search_impl(&self.child(h, i)?, min, max, visitor, found)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn min_dist_sq(&self, h: &NodeHandle, i: usize, point: &[E; D]) -> Result<i64> {
        let (min, max) = self.node(h)?.get_child_bounds(i)?;
        let mut distance = 0;
        for d in 0..D {
            let diff = if point[d] < min[d] {
                sub(min[d].widen(), point[d].widen())?
            } else if point[d] > max[d] {
                sub(point[d].widen(), max[d].widen())?
            } else {
                continue;
            };
            distance = add(distance, mul(diff, diff)?)?;
        }
        Ok(distance)
    }
    /// Clears outputs even for k<=0/empty. Queue ties follow the measured
    /// libstdc++ heap operations, without sorting results by data or address.
    pub fn nearest_neighbors(
        &self,
        point: [E; D],
        k: i32,
        results: &mut Vec<(i64, T)>,
    ) -> Result<()> {
        results.clear();
        self.ready()?;
        let Some(root) = self.root.as_ref() else {
            return Ok(());
        };
        if k <= 0 {
            return Ok(());
        }
        let mut queue: Vec<(i64, NodeHandle, usize)> = Vec::new();
        let cmp = |a: &(i64, NodeHandle, usize), b: &(i64, NodeHandle, usize)| a.0 > b.0;
        let push =
            |queue: &mut Vec<(i64, NodeHandle, usize)>, h: &NodeHandle, i: usize| -> Result<()> {
                let distance = self.min_dist_sq(h, i, &point)?;
                queue.try_reserve(1).map_err(|_| Error::Resource)?;
                queue.push((distance, h.clone(), i));
                let last = queue.len() - 1;
                order::heap_push(queue, last, &cmp)
            };
        for i in 0..usize::try_from(self.node(root)?.count).map_err(|_| Error::Invariant)? {
            push(&mut queue, root, i)?;
        }
        while !queue.is_empty() && results.len() < k as usize {
            let len = queue.len();
            if len > 1 {
                order::heap_pop(&mut queue, len, &cmp)?;
            }
            let (distance, h, i) = queue.pop().ok_or(Error::Invariant)?;
            if self.node(&h)?.is_leaf() {
                results.try_reserve(1).map_err(|_| Error::Resource)?;
                results.push((distance, self.data(&h, i)?));
            } else {
                let child = self.child(&h, i)?;
                for i in
                    0..usize::try_from(self.node(&child)?.count).map_err(|_| Error::Invariant)?
                {
                    push(&mut queue, &child, i)?;
                }
            }
        }
        Ok(())
    }
}
