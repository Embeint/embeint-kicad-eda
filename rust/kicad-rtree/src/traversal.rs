// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{add, mul, order, sub, Coord, DynamicRtree, Error, NodeData, Result};
use kicad_rtree_node::{NodeHandle, RtreeNode, SlabArena};

/// Owned traversal state, matching the reference's copied payload and node/slot
/// stack. It can survive defined mutations; advance resolves handles in the live
/// arena and refuses actual use-after-free instead of dereferencing ended objects.
#[derive(Clone)]
pub struct TreeCursor<T: NodeData, E: Coord, const D: usize, const F: usize> {
    stack: Vec<(NodeHandle, usize)>,
    query: Option<([E; D], [E; D])>,
    current: Option<T>,
    at_end: bool,
    max_stack: usize,
}
impl<T: NodeData, E: Coord, const D: usize, const F: usize> Default for TreeCursor<T, E, D, F> {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            query: None,
            current: None,
            at_end: true,
            max_stack: 65_536,
        }
    }
}
impl<T: NodeData, E: Coord, const D: usize, const F: usize> TreeCursor<T, E, D, F> {
    pub fn from_root(
        arena: &SlabArena<RtreeNode<T, E, D, F>>,
        root: Option<&NodeHandle>,
        query: Option<([E; D], [E; D])>,
        max_stack: usize,
    ) -> Result<Self> {
        let mut it = Self {
            query,
            max_stack,
            ..Self::default()
        };
        if let Some(root) = root {
            if arena.node(root)?.count > 0 {
                if max_stack == 0 {
                    return Err(Error::Resource);
                }
                it.stack.try_reserve(1).map_err(|_| Error::Resource)?;
                it.stack.push((root.clone(), 0));
                it.advance(arena)?;
            }
        }
        Ok(it)
    }
    pub fn at_end(&self) -> bool {
        self.at_end
    }
    /// None represents the default/null DATATYPE. It is a valid current payload
    /// even when at_end is false, for an external node containing a null pointer.
    pub fn current(&self) -> Option<&T> {
        self.current.as_ref()
    }
    pub fn advance(&mut self, arena: &SlabArena<RtreeNode<T, E, D, F>>) -> Result<()> {
        while let Some((h, i)) = self.stack.last().cloned() {
            let n = arena.node(&h)?;
            if i >= n.count.max(0) as usize {
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
                self.current = n.data(i)?.cloned();
                self.at_end = false;
                return Ok(());
            }
            if self.stack.len() >= self.max_stack {
                return Err(Error::Resource);
            }
            self.stack.try_reserve(1).map_err(|_| Error::Resource)?;
            self.stack
                .push((n.child(i)?.cloned().ok_or(Error::Invariant)?, 0));
        }
        self.at_end = true;
        Ok(())
    }
}
impl<T: NodeData, E: Coord, const D: usize, const F: usize> PartialEq for TreeCursor<T, E, D, F> {
    fn eq(&self, other: &Self) -> bool {
        self.at_end == other.at_end
    }
}
/// Borrowed convenience form of Iterator/SearchIterator. The explicit detach()
/// form permits mutation between increments, as in the reference. Equality tests
/// only end flags, and exhaustion retains the most recently copied payload.
#[derive(Clone)]
pub struct TreeIterator<'a, T: NodeData, E: Coord, const D: usize, const F: usize> {
    arena: Option<&'a SlabArena<RtreeNode<T, E, D, F>>>,
    cursor: TreeCursor<T, E, D, F>,
}
impl<T: NodeData, E: Coord, const D: usize, const F: usize> Default
    for TreeIterator<'_, T, E, D, F>
{
    fn default() -> Self {
        Self {
            arena: None,
            cursor: TreeCursor::default(),
        }
    }
}
impl<'a, T: NodeData, E: Coord, const D: usize, const F: usize> TreeIterator<'a, T, E, D, F> {
    fn new(
        tree: &'a DynamicRtree<T, E, D, F>,
        query: Option<([E; D], [E; D])>,
        end: bool,
    ) -> Result<Self> {
        tree.ready()?;
        if end {
            return Ok(Self::default());
        }
        Self::from_root_with_budget(
            &tree.arena,
            tree.root.as_ref(),
            query,
            tree.budget.max_nodes,
        )
    }
    pub fn from_root(
        arena: &'a SlabArena<RtreeNode<T, E, D, F>>,
        root: Option<&NodeHandle>,
        query: Option<([E; D], [E; D])>,
    ) -> Result<Self> {
        Self::from_root_with_budget(arena, root, query, 65_536)
    }
    pub fn from_root_with_budget(
        arena: &'a SlabArena<RtreeNode<T, E, D, F>>,
        root: Option<&NodeHandle>,
        query: Option<([E; D], [E; D])>,
        max_stack: usize,
    ) -> Result<Self> {
        Ok(Self {
            arena: Some(arena),
            cursor: TreeCursor::from_root(arena, root, query, max_stack)?,
        })
    }
    pub fn detach(self) -> TreeCursor<T, E, D, F> {
        self.cursor
    }
    pub fn at_end(&self) -> bool {
        self.cursor.at_end()
    }
    pub fn current(&self) -> Option<&T> {
        self.cursor.current()
    }
    pub fn advance(&mut self) -> Result<()> {
        if let Some(arena) = self.arena {
            self.cursor.advance(arena)
        } else {
            Ok(())
        }
    }
}
impl<T: NodeData, E: Coord, const D: usize, const F: usize> PartialEq
    for TreeIterator<'_, T, E, D, F>
{
    fn eq(&self, other: &Self) -> bool {
        self.cursor == other.cursor
    }
}
/// Owned SearchRange state. Saving a range does not traverse its root; begin()
/// after a defined mutation sees that saved root, including when it is no longer
/// the tree's current root. A null saved root stays an empty range after inserts.
#[derive(Clone)]
pub struct SearchRangeSnapshot<E: Coord, const D: usize> {
    root: Option<NodeHandle>,
    min: [E; D],
    max: [E; D],
    max_stack: usize,
}
impl<E: Coord, const D: usize> SearchRangeSnapshot<E, D> {
    pub fn begin<T: NodeData, const F: usize>(
        &self,
        arena: &SlabArena<RtreeNode<T, E, D, F>>,
    ) -> Result<TreeCursor<T, E, D, F>> {
        TreeCursor::from_root(
            arena,
            self.root.as_ref(),
            Some((self.min, self.max)),
            self.max_stack,
        )
    }
    pub fn empty<T: NodeData, const F: usize>(
        &self,
        arena: &SlabArena<RtreeNode<T, E, D, F>>,
    ) -> Result<bool> {
        Ok(self.begin(arena)?.at_end())
    }
}
/// A lazy borrowed SearchRange; constructing it copies bounds, without searching.
#[derive(Clone)]
pub struct SearchRange<'a, T: NodeData, E: Coord, const D: usize, const F: usize> {
    arena: &'a SlabArena<RtreeNode<T, E, D, F>>,
    root: Option<NodeHandle>,
    max_stack: usize,
    min: [E; D],
    max: [E; D],
}
impl<'a, T: NodeData, E: Coord, const D: usize, const F: usize> SearchRange<'a, T, E, D, F> {
    pub fn from_root(
        arena: &'a SlabArena<RtreeNode<T, E, D, F>>,
        root: Option<&NodeHandle>,
        min: [E; D],
        max: [E; D],
    ) -> Self {
        Self {
            arena,
            root: root.cloned(),
            max_stack: 65_536,
            min,
            max,
        }
    }
    pub fn detach(self) -> SearchRangeSnapshot<E, D> {
        SearchRangeSnapshot {
            root: self.root,
            min: self.min,
            max: self.max,
            max_stack: self.max_stack,
        }
    }
    pub fn begin(&self) -> Result<TreeIterator<'a, T, E, D, F>> {
        TreeIterator::from_root_with_budget(
            self.arena,
            self.root.as_ref(),
            Some((self.min, self.max)),
            self.max_stack,
        )
    }
    pub fn end(&self) -> Result<TreeIterator<'a, T, E, D, F>> {
        Ok(TreeIterator::default())
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
        let mut range = SearchRange::from_root(&self.arena, self.root.as_ref(), min, max);
        range.max_stack = self.budget.max_nodes;
        Ok(range)
    }
    pub fn begin_range(&self, range: &SearchRangeSnapshot<E, D>) -> Result<TreeCursor<T, E, D, F>> {
        self.ready()?;
        range.begin(&self.arena)
    }
    pub fn cursor(&self) -> Result<TreeCursor<T, E, D, F>> {
        Ok(self.begin()?.detach())
    }
    pub fn overlapping_cursor(&self, min: [E; D], max: [E; D]) -> Result<TreeCursor<T, E, D, F>> {
        Ok(self.overlapping(min, max)?.begin()?.detach())
    }
    pub fn advance_cursor(&self, cursor: &mut TreeCursor<T, E, D, F>) -> Result<()> {
        self.ready()?;
        cursor.advance(&self.arena)
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
        let mut stack = Vec::new();
        stack.try_reserve(1).map_err(|_| Error::Resource)?;
        stack.push((h.clone(), self.node(h)?.child_overlap_mask(min, max)?));
        while let Some((h, mask)) = stack.last_mut() {
            if *mask == 0 {
                stack.pop();
                continue;
            }
            let i = mask.trailing_zeros() as usize;
            *mask &= *mask - 1;
            if self.node(h)?.is_leaf() {
                *found = found.checked_add(1).ok_or(Error::Overflow)?;
                if !visitor(self.node(h)?.data(i)?.ok_or(Error::Invariant)?) {
                    return Ok(false);
                }
            } else {
                let child = self.child(h, i)?;
                let mask = self.node(&child)?.child_overlap_mask(min, max)?;
                stack.try_reserve(1).map_err(|_| Error::Resource)?;
                stack.push((child, mask));
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
