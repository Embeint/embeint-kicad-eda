// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{error::push, Coord, DynamicRtree, Error, NodeData, NodeHandle};

/// Lazy all-data or overlap iterator. Its cached current value, increment past
/// end, and equality-by-atEnd match the unusual reference iterator contracts.
/// The immutable borrow prevents invalidation by mutations or allocator moves.
#[derive(Clone)]
pub struct Cursor<'a, T: NodeData, E: Coord, const N: usize, const M: usize> {
    tree: &'a DynamicRtree<T, E, N, M>,
    stack: Vec<(NodeHandle, usize)>,
    query: Option<([E; N], [E; N])>,
    current: Option<T>,
    at_end: bool,
}
impl<'a, T: NodeData, E: Coord, const N: usize, const M: usize> Cursor<'a, T, E, N, M> {
    pub(crate) fn new(
        tree: &'a DynamicRtree<T, E, N, M>,
        query: Option<([E; N], [E; N])>,
    ) -> Result<Self, Error> {
        let mut cursor = Self {
            tree,
            stack: Vec::new(),
            query,
            current: None,
            at_end: true,
        };
        if let Some(root) = &tree.root {
            if tree.n(root)?.count > 0 {
                push(&mut cursor.stack, (root.clone(), 0))?;
                cursor.advance()?;
            }
        }
        Ok(cursor)
    }
    pub(crate) fn end(tree: &'a DynamicRtree<T, E, N, M>) -> Self {
        Self {
            tree,
            stack: Vec::new(),
            query: None,
            current: None,
            at_end: true,
        }
    }
    pub fn is_end(&self) -> bool {
        self.at_end
    }
    /// Dereference/arrow access the cached value even at end (last visited item,
    /// or null for a default/empty iterator), as in the C++ iterator.
    pub fn current(&self) -> &Option<T> {
        &self.current
    }
    pub fn advance(&mut self) -> Result<(), Error> {
        while let Some((h, i)) = self.stack.last().cloned() {
            let node = self.tree.n(&h)?;
            if i >= node.count as usize {
                self.stack.pop();
                continue;
            }
            self.stack.last_mut().ok_or(Error::Invariant)?.1 += 1;
            if let Some((min, max)) = &self.query {
                if !node.child_overlaps(i, min, max)? {
                    continue;
                }
            }
            if node.is_leaf() {
                self.current = node.data(i)?.cloned();
                self.at_end = false;
                return Ok(());
            }
            if self.stack.len() >= 65 {
                return Err(Error::ResourceLimit);
            }
            push(&mut self.stack, (self.tree.child(&h, i)?, 0))?;
        }
        self.at_end = true;
        Ok(())
    }
}
impl<T: NodeData, E: Coord, const N: usize, const M: usize> PartialEq for Cursor<'_, T, E, N, M> {
    fn eq(&self, other: &Self) -> bool {
        self.at_end == other.at_end
    }
}
impl<T: NodeData, E: Coord, const N: usize, const M: usize> Eq for Cursor<'_, T, E, N, M> {}

pub struct SearchRange<'a, T: NodeData, E: Coord, const N: usize, const M: usize> {
    tree: &'a DynamicRtree<T, E, N, M>,
    min: [E; N],
    max: [E; N],
}
impl<'a, T: NodeData, E: Coord, const N: usize, const M: usize> SearchRange<'a, T, E, N, M> {
    pub(crate) fn new(tree: &'a DynamicRtree<T, E, N, M>, min: [E; N], max: [E; N]) -> Self {
        Self { tree, min, max }
    }
    pub fn begin(&self) -> Result<Cursor<'a, T, E, N, M>, Error> {
        Cursor::new(self.tree, Some((self.min, self.max)))
    }
    pub fn end(&self) -> Cursor<'a, T, E, N, M> {
        Cursor::end(self.tree)
    }
    pub fn is_empty(&self) -> Result<bool, Error> {
        Ok(self.begin()? == self.end())
    }
}
