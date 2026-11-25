// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! Safe port of KiCad's pinned `dynamic_rtree.h`. Nodes and storage are the
//! accepted `kicad-rtree-node` types. Reference ordering/accounting targets Linux
//! x86-64 libstdc++; coordinates remain integers throughout spatial predicates.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod error;
mod iter;
/// Shared libstdc++ ordering contract for the COW/packed sibling unit.
pub mod order;
pub use error::Error;
use error::{add, mul, push, reserve, sub};
pub use iter::{Cursor, SearchRange};
use kicad_rtree_node::{hilbert_nd, AllocatorId, Payload, SlabArena};
pub use kicad_rtree_node::{Coord, NodeData, NodeHandle, RtreeNode};

type Node<T, E, const N: usize, const M: usize> = RtreeNode<T, E, N, M>;

#[derive(Clone, Debug, PartialEq)]
pub struct BulkEntry<T, E, const N: usize> {
    pub min: [E; N],
    pub max: [E; N],
    pub data: Option<T>,
}
#[derive(Clone)]
struct Entry<T, E, const N: usize> {
    min: [E; N],
    max: [E; N],
    insert_min: [E; N],
    insert_max: [E; N],
    payload: Payload<T>,
}
impl<T: Clone, E: Copy, const N: usize> Entry<T, E, N> {
    fn leaf(min: [E; N], max: [E; N], data: Option<T>) -> Self {
        Self {
            min,
            max,
            insert_min: min,
            insert_max: max,
            payload: data.map_or(Payload::Null, Payload::Data),
        }
    }
    fn internal(min: [E; N], max: [E; N], child: NodeHandle) -> Self {
        Self {
            min,
            max,
            insert_min: min,
            insert_max: max,
            payload: Payload::Child(child),
        }
    }
    fn data(&self) -> Result<Option<T>, Error> {
        match &self.payload {
            Payload::Null => Ok(None),
            Payload::Data(x) => Ok(Some(x.clone())),
            _ => Err(Error::Invariant),
        }
    }
    fn child(&self) -> Result<NodeHandle, Error> {
        match &self.payload {
            Payload::Child(x) => Ok(x.clone()),
            _ => Err(Error::Invariant),
        }
    }
}

/// Diagnostics count real algorithm branches, never alter tree decisions.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Branches {
    pub leaf_splits: usize,
    pub internal_splits: usize,
    pub forced_reinserts: usize,
    pub nonroot_reinserts: usize,
    pub root_splits: usize,
    pub root_condensations: usize,
    pub leaf_orphans: usize,
    pub internal_orphans: usize,
    pub empty_child_frees: usize,
    pub fallback_removals: usize,
}

/// Unique tree ownership. Rust moves transfer the arena and its handles unchanged.
/// `move_construct`/`move_assign` also expose C++'s reusable empty source state.
/// Immutable cursors borrow the tree, preventing mutation while they are live.
#[derive(Debug)]
pub struct DynamicRtree<T: NodeData, E: Coord, const N: usize = 2, const M: usize = 16> {
    arena: SlabArena<Node<T, E, N, M>>,
    allocator: AllocatorId,
    root: Option<NodeHandle>,
    count: usize,
    max_entries: usize,
    interrupted: bool,
    branches: Branches,
}
impl<T: NodeData, E: Coord, const N: usize, const M: usize> DynamicRtree<T, E, N, M> {
    /// Resource budget is explicit and can be raised by the host. Invalid bounds
    /// are accepted wherever the reference has defined arithmetic; no normalizing.
    pub fn new(max_entries: usize) -> Result<Self, Error> {
        if N == 0 || M < 2 || M > i32::MAX as usize / 3 {
            return Err(Error::InvalidTemplate);
        }
        let mut arena = SlabArena::new();
        let allocator = arena.new_allocator()?;
        Ok(Self {
            arena,
            allocator,
            root: None,
            count: 0,
            max_entries,
            interrupted: false,
            branches: Branches::default(),
        })
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn branches(&self) -> &Branches {
        &self.branches
    }
    pub fn memory_usage(&self) -> Result<u64, Error> {
        Ok(self.arena.memory_usage(&self.allocator)?)
    }
    pub fn root_handle(&self) -> Option<NodeHandle> {
        self.root.clone()
    }
    /// Read-only handle access for diagnostics and transferred-handle validation.
    pub fn node(&self, h: &NodeHandle) -> Result<&Node<T, E, N, M>, Error> {
        self.ready()?;
        Ok(self.arena.node(h)?)
    }
    fn ready(&self) -> Result<(), Error> {
        if self.interrupted {
            Err(Error::InterruptedMutation)
        } else {
            Ok(())
        }
    }
    fn n(&self, h: &NodeHandle) -> Result<&Node<T, E, N, M>, Error> {
        Ok(self.arena.node(h)?)
    }
    fn nm(&mut self, h: &NodeHandle) -> Result<&mut Node<T, E, N, M>, Error> {
        Ok(self.arena.node_mut(h)?)
    }
    fn root(&self) -> Result<NodeHandle, Error> {
        self.root.clone().ok_or(Error::Invariant)
    }
    fn child(&self, h: &NodeHandle, i: usize) -> Result<NodeHandle, Error> {
        self.n(h)?.child(i)?.cloned().ok_or(Error::Invariant)
    }
    fn alloc(&mut self, level: i32) -> Result<NodeHandle, Error> {
        let h = self.arena.allocate(&self.allocator)?;
        self.nm(&h)?.level = level;
        Ok(h)
    }
    fn free(&mut self, h: &NodeHandle) -> Result<(), Error> {
        Ok(self.arena.free(&self.allocator, Some(h))?)
    }
    fn entries(&self, h: &NodeHandle) -> Result<Vec<Entry<T, E, N>>, Error> {
        let node = self.n(h)?;
        let mut out = Vec::new();
        reserve(&mut out, M + 1)?;
        for i in 0..node.count as usize {
            let (min, max) = node.get_child_bounds(i)?;
            let (insert_min, insert_max) = node.get_insert_bounds(i)?;
            out.push(Entry {
                min,
                max,
                insert_min,
                insert_max,
                payload: node.payload(i)?.clone(),
            });
        }
        Ok(out)
    }
    fn append(&mut self, h: &NodeHandle, e: &Entry<T, E, N>) -> Result<(), Error> {
        let node = self.nm(h)?;
        let i = usize::try_from(node.count).map_err(|_| Error::Invariant)?;
        node.set_child_bounds(i, &e.min, &e.max)?;
        if node.is_leaf() {
            node.set_insert_bounds(i, &e.insert_min, &e.insert_max)?;
            node.set_data(i, e.data()?)?;
        } else {
            node.set_child(i, Some(e.child()?))?;
        }
        node.count += 1;
        Ok(())
    }
    fn rebuild(&mut self, h: &NodeHandle, es: &[Entry<T, E, N>]) -> Result<(), Error> {
        self.nm(h)?.count = 0;
        for e in es {
            self.append(h, e)?;
        }
        Ok(())
    }
    fn remove_all_nodes(&mut self, h: Option<NodeHandle>) -> Result<(), Error> {
        let Some(h) = h else { return Ok(()) };
        // Explicit postorder retains the reference free-list ordering without
        // input-driven native stack recursion.
        let mut stack = Vec::new();
        push(&mut stack, (h, false))?;
        while let Some((node, visited)) = stack.pop() {
            if visited || self.n(&node)?.is_leaf() {
                self.free(&node)?;
                continue;
            }
            push(&mut stack, (node.clone(), true))?;
            for i in (0..self.n(&node)?.count as usize).rev() {
                push(&mut stack, (self.child(&node, i)?, false))?;
            }
        }
        Ok(())
    }
    /// Clear retains allocator pages and free-list capacity, exactly as RemoveAll.
    /// After a refused undefined mutation, reset releases the entire arena instead.
    pub fn clear(&mut self) -> Result<(), Error> {
        if self.interrupted {
            *self = Self::new(self.max_entries)?;
            return Ok(());
        }
        self.remove_all_nodes(self.root.clone())?;
        self.root = None;
        self.count = 0;
        Ok(())
    }
    pub fn move_construct(source: &mut Self) -> Result<Self, Error> {
        let mut result = Self::new(source.max_entries)?;
        core::mem::swap(&mut result, source);
        Ok(result)
    }
    pub fn move_assign(&mut self, source: &mut Self) -> Result<(), Error> {
        // Safe Rust cannot produce two mutable references to the same tree. The
        // native self-assignment guard is exposed separately as move_assign_self.
        let moved = Self::move_construct(source)?;
        *self = moved;
        Ok(())
    }
    pub fn move_assign_self(&mut self) {}
    pub fn insert(&mut self, min: [E; N], max: [E; N], data: Option<T>) -> Result<(), Error> {
        self.ready()?;
        if self.count >= self.max_entries {
            return Err(Error::ResourceLimit);
        }
        self.interrupted = true;
        if self.root.is_none() {
            self.root = Some(self.alloc(0)?);
        }
        self.insert_impl(Entry::leaf(min, max, data), &mut 0)?;
        self.count = self.count.checked_add(1).ok_or(Error::ResourceLimit)?;
        self.interrupted = false;
        Ok(())
    }
    fn insert_impl(&mut self, e: Entry<T, E, N>, levels: &mut u32) -> Result<(), Error> {
        let (leaf, path) = self.choose(&e.min, &e.max, None)?;
        if !self.n(&leaf)?.is_full() {
            self.append(&leaf, &e)?;
            self.adjust(&path, Some(leaf))
        } else {
            self.overflow(leaf, e, path, levels)
        }
    }
    fn choose(
        &self,
        min: &[E; N],
        max: &[E; N],
        target: Option<i32>,
    ) -> Result<(NodeHandle, Vec<NodeHandle>), Error> {
        let mut path = Vec::new();
        let mut h = self.root()?;
        while self.n(&h)?.level > target.unwrap_or(0) {
            if path.len() >= 64 {
                return Err(Error::ResourceLimit);
            }
            push(&mut path, h.clone())?;
            let node = self.n(&h)?;
            let mut best = 0;
            let mut best_key = (i64::MAX, i64::MAX, i64::MAX);
            for i in 0..node.count as usize {
                let overlap = if target.is_none() && node.level == 1 {
                    {
                        let before = self.overlap(&h, i, None)?;
                        let after = self.overlap(&h, i, Some((min, max)))?;
                        sub(after, before)?
                    }
                } else {
                    0
                };
                let key = (
                    overlap,
                    node.child_enlargement(i, min, max)?,
                    node.child_area(i)?,
                );
                if key < best_key {
                    best = i;
                    best_key = key;
                }
            }
            h = self.child(&h, best)?;
        }
        Ok((h, path))
    }
    fn overlap(
        &self,
        h: &NodeHandle,
        i: usize,
        enlarge: Option<(&[E; N], &[E; N])>,
    ) -> Result<i64, Error> {
        let node = self.n(h)?;
        let (mut min, mut max) = node.get_child_bounds(i)?;
        if let Some((lo, hi)) = enlarge {
            for d in 0..N {
                min[d] = min[d].min(lo[d]);
                max[d] = max[d].max(hi[d]);
            }
        }
        let mut sum = 0;
        for j in 0..node.count as usize {
            if i != j {
                sum = add(sum, node.child_overlap_area(j, &min, &max)?)?;
            }
        }
        Ok(sum)
    }
    fn find_child(&self, parent: &NodeHandle, child: &NodeHandle) -> Result<Option<usize>, Error> {
        for i in 0..self.n(parent)?.count as usize {
            if self.n(parent)?.child(i)? == Some(child) {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
    fn update(&mut self, parent: &NodeHandle, child: &NodeHandle) -> Result<(), Error> {
        if let Some(i) = self.find_child(parent, child)? {
            let (lo, hi) = self.n(child)?.compute_enclosing_bounds()?;
            self.nm(parent)?.set_child_bounds(i, &lo, &hi)?;
        }
        Ok(())
    }
    fn adjust(&mut self, path: &[NodeHandle], mut child: Option<NodeHandle>) -> Result<(), Error> {
        for parent in path.iter().rev() {
            if let Some(h) = child {
                self.update(parent, &h)?;
            }
            child = Some(parent.clone());
        }
        Ok(())
    }
    fn overflow(
        &mut self,
        h: NodeHandle,
        e: Entry<T, E, N>,
        path: Vec<NodeHandle>,
        levels: &mut u32,
    ) -> Result<(), Error> {
        let level = self.n(&h)?.level;
        if level >= 32 {
            return self.split(h, e, path, levels, false);
        }
        let mask = 1u32.checked_shl(level as u32).ok_or(Error::Invariant)?;
        if *levels & mask == 0 {
            *levels |= mask;
            self.forced(h, e, path, levels)
        } else {
            self.split(h, e, path, levels, false)
        }
    }
    fn forced(
        &mut self,
        h: NodeHandle,
        e: Entry<T, E, N>,
        path: Vec<NodeHandle>,
        levels: &mut u32,
    ) -> Result<(), Error> {
        let overflow = e.clone();
        let mut es = self.entries(&h)?;
        push(&mut es, e)?;
        let (lo, hi) = self.n(&h)?.compute_enclosing_bounds()?;
        let center: [f64; N] =
            core::array::from_fn(|d| (lo[d].widen() as f64 + hi[d].widen() as f64) / 2.0);
        let mut ranked = Vec::new();
        reserve(&mut ranked, es.len())?;
        for entry in es {
            let mut dist = 0;
            for (d, node_center) in center.iter().enumerate() {
                let ec = (entry.min[d].widen() as f64 + entry.max[d].widen() as f64) / 2.0;
                let diff = ec - node_center;
                let square = diff * diff;
                // C++ casts each square before summing. 2^63 is already outside
                // int64_t; i64::MAX rounds to that same double.
                if !(0.0..9223372036854775808.0).contains(&square) {
                    return Err(Error::Conversion);
                }
                dist = add(dist, square as i64)?;
            }
            ranked.push((dist, entry));
        }
        order::sort(&mut ranked, |a, b| a.0 > b.0)?;
        let take = (M * 3 / 10).min(ranked.len() - M * 2 / 5);
        if take == 0 {
            return self.split(h, overflow, path, levels, false);
        }
        self.branches.forced_reinserts += 1;
        if !path.is_empty() {
            self.branches.nonroot_reinserts += 1;
        }
        self.nm(&h)?.count = 0;
        for (_, entry) in ranked.iter().skip(take) {
            self.append(&h, entry)?;
        }
        self.adjust(&path, Some(h.clone()))?;
        for (_, entry) in ranked.into_iter().take(take) {
            if self.n(&h)?.is_leaf() {
                self.insert_impl(
                    Entry::leaf(entry.insert_min, entry.insert_max, entry.data()?),
                    levels,
                )?;
            } else {
                self.reinsert_node(entry, self.n(&h)?.level - 1, levels)?;
            }
        }
        Ok(())
    }
    fn reinsert_node(
        &mut self,
        e: Entry<T, E, N>,
        level: i32,
        levels: &mut u32,
    ) -> Result<(), Error> {
        let (target, path) = self.choose(&e.min, &e.max, Some(level + 1))?;
        if !self.n(&target)?.is_full() {
            self.append(&target, &e)?;
            self.adjust(&path, Some(target))
        } else {
            self.split(target, e, path, levels, true)
        }
    }
    fn group_bounds(es: &[Entry<T, E, N>]) -> ([E; N], [E; N]) {
        let mut lo = [E::MAX; N];
        let mut hi = [E::LOWEST; N];
        for e in es {
            for d in 0..N {
                lo[d] = lo[d].min(e.min[d]);
                hi[d] = hi[d].max(e.max[d]);
            }
        }
        (lo, hi)
    }
    fn split_overlap(es: &[Entry<T, E, N>], k: usize) -> Result<i64, Error> {
        let (a, b) = es.split_at_checked(k).ok_or(Error::Invariant)?;
        let (alo, ahi) = Self::group_bounds(a);
        let (blo, bhi) = Self::group_bounds(b);
        let mut overlap = 1;
        for d in 0..N {
            let lo = alo[d].max(blo[d]);
            let hi = ahi[d].min(bhi[d]);
            if lo >= hi {
                return Ok(0);
            }
            overlap = mul(overlap, sub(hi.widen(), lo.widen())?)?;
        }
        Ok(overlap)
    }
    fn split_perimeters(es: &[Entry<T, E, N>]) -> Result<i64, Error> {
        let min = M * 2 / 5;
        let mut sum = 0;
        for k in min..=es.len() - min {
            let (a, b) = es.split_at_checked(k).ok_or(Error::Invariant)?;
            for group in [a, b] {
                let (lo, hi) = Self::group_bounds(group);
                let mut perimeter = 0;
                for d in 0..N {
                    perimeter = add(perimeter, sub(hi[d].widen(), lo[d].widen())?)?;
                }
                sum = add(sum, mul(2, perimeter)?)?;
            }
        }
        Ok(sum)
    }
    fn best_split(es: &[Entry<T, E, N>]) -> Result<usize, Error> {
        let min = M * 2 / 5;
        let mut best = min;
        let mut key = (i64::MAX, i64::MAX);
        for k in min..=es.len() - min {
            let overlap = Self::split_overlap(es, k)?;
            let mut area_sum = 0;
            let (a, b) = es.split_at_checked(k).ok_or(Error::Invariant)?;
            for group in [a, b] {
                let (lo, hi) = Self::group_bounds(group);
                let mut area = 1;
                for d in 0..N {
                    area = mul(area, sub(hi[d].widen(), lo[d].widen())?)?;
                }
                area_sum = add(area_sum, area)?;
            }
            if (overlap, area_sum) < key {
                best = k;
                key = (overlap, area_sum);
            }
        }
        Ok(best)
    }
    fn split(
        &mut self,
        h: NodeHandle,
        e: Entry<T, E, N>,
        mut path: Vec<NodeHandle>,
        levels: &mut u32,
        internal: bool,
    ) -> Result<(), Error> {
        let _ = levels;
        let mut es = self.entries(&h)?;
        push(&mut es, e)?;
        let mut axis_best = 0;
        let mut sum_best = i64::MAX;
        for axis in 0..N {
            order::sort(&mut es, |a, b| {
                if internal {
                    a.min[axis] < b.min[axis]
                } else {
                    (a.min[axis], a.max[axis]) < (b.min[axis], b.max[axis])
                }
            })?;
            let mut sum = Self::split_perimeters(&es)?;
            if !internal {
                order::sort(&mut es, |a, b| {
                    (a.max[axis], a.min[axis]) < (b.max[axis], b.min[axis])
                })?;
                sum = add(sum, Self::split_perimeters(&es)?)?;
            }
            if sum < sum_best {
                axis_best = axis;
                sum_best = sum;
            }
        }
        order::sort(&mut es, |a, b| {
            if internal {
                a.min[axis_best] < b.min[axis_best]
            } else {
                (a.min[axis_best], a.max[axis_best]) < (b.min[axis_best], b.max[axis_best])
            }
        })?;
        let mut k = Self::best_split(&es)?;
        if !internal {
            let mut by_max = es.clone();
            order::sort(&mut by_max, |a, b| {
                (a.max[axis_best], a.min[axis_best]) < (b.max[axis_best], b.min[axis_best])
            })?;
            let km = Self::best_split(&by_max)?;
            if Self::split_overlap(&by_max, km)? < Self::split_overlap(&es, k)? {
                es = by_max;
                k = km;
            }
        }
        if self.n(&h)?.is_leaf() {
            self.branches.leaf_splits += 1;
        } else {
            self.branches.internal_splits += 1;
        }
        let sibling = self.alloc(self.n(&h)?.level)?;
        let (a, b) = es.split_at_checked(k).ok_or(Error::Invariant)?;
        self.rebuild(&h, a)?;
        self.rebuild(&sibling, b)?;
        let (lo, hi) = self.n(&sibling)?.compute_enclosing_bounds()?;
        if let Some(parent) = path.pop() {
            self.update(&parent, &h)?;
            let e = Entry::internal(lo, hi, sibling);
            if !self.n(&parent)?.is_full() {
                self.append(&parent, &e)?;
                self.adjust(&path, Some(parent))
            } else {
                self.split(parent, e, path, levels, true)
            }
        } else {
            let level = self
                .n(&self.root()?)?
                .level
                .checked_add(1)
                .ok_or(Error::Overflow)?;
            if level > 64 {
                return Err(Error::ResourceLimit);
            }
            let root = self.alloc(level)?;
            let (nlo, nhi) = self.n(&h)?.compute_enclosing_bounds()?;
            self.append(&root, &Entry::internal(nlo, nhi, h))?;
            self.append(&root, &Entry::internal(lo, hi, sibling))?;
            self.root = Some(root);
            self.branches.root_splits += 1;
            Ok(())
        }
    }
    pub fn remove(&mut self, min: [E; N], max: [E; N], data: Option<T>) -> Result<bool, Error> {
        self.ready()?;
        let Some(root) = self.root.clone() else {
            return Ok(false);
        };
        self.interrupted = true;
        let mut orphans = Vec::new();
        let mut found = self.remove_impl(&root, &min, &max, &data, &mut orphans, 0)?;
        if !found {
            orphans.clear();
            self.branches.fallback_removals += 1;
            found =
                self.remove_impl(&root, &[E::LOWEST; N], &[E::MAX; N], &data, &mut orphans, 0)?;
        }
        if found {
            self.count = self.count.checked_sub(1).ok_or(Error::Invariant)?;
            self.reinsert_orphans(orphans)?;
            self.condense_root()?;
        }
        self.interrupted = false;
        Ok(found)
    }
    fn remove_impl(
        &mut self,
        h: &NodeHandle,
        min: &[E; N],
        max: &[E; N],
        data: &Option<T>,
        orphans: &mut Vec<NodeHandle>,
        depth: usize,
    ) -> Result<bool, Error> {
        if depth > 64 {
            return Err(Error::ResourceLimit);
        }
        let leaf = self.n(h)?.is_leaf();
        let count = self.n(h)?.count as usize;
        for i in 0..count {
            if leaf && self.n(h)?.data(i)? != data.as_ref() {
                continue;
            }
            if !self.n(h)?.child_overlaps(i, min, max)? {
                continue;
            }
            if leaf {
                self.nm(h)?.remove_child(i as i32)?;
                return Ok(true);
            }
            let child = self.child(h, i)?;
            if self.remove_impl(&child, min, max, data, orphans, depth + 1)? {
                let count = self.n(&child)?.count;
                if count > 0 {
                    let (lo, hi) = self.n(&child)?.compute_enclosing_bounds()?;
                    self.nm(h)?.set_child_bounds(i, &lo, &hi)?;
                    // Pinned quirk: test the parent against root, not the child.
                    if count < (M * 2 / 5) as i32 && Some(h) != self.root.as_ref() {
                        push(orphans, child.clone())?;
                        self.nm(h)?.remove_child(i as i32)?;
                        if self.n(&child)?.is_leaf() {
                            self.branches.leaf_orphans += 1;
                        } else {
                            self.branches.internal_orphans += 1;
                        }
                    }
                } else {
                    self.free(&child)?;
                    self.nm(h)?.remove_child(i as i32)?;
                    self.branches.empty_child_frees += 1;
                }
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn reinsert_orphans(&mut self, orphans: Vec<NodeHandle>) -> Result<(), Error> {
        for orphan in orphans {
            let leaf = self.n(&orphan)?.is_leaf();
            let level = self.n(&orphan)?.level;
            for e in self.entries(&orphan)? {
                if leaf {
                    self.insert_impl(Entry::leaf(e.insert_min, e.insert_max, e.data()?), &mut 0)?;
                } else {
                    self.reinsert_node(e, level - 1, &mut 0)?;
                }
            }
            self.free(&orphan)?;
        }
        Ok(())
    }
    fn condense_root(&mut self) -> Result<(), Error> {
        while let Some(root) = self.root.clone() {
            if self.n(&root)?.is_internal() && self.n(&root)?.count == 1 {
                self.root = Some(self.child(&root, 0)?);
                self.free(&root)?;
                self.branches.root_condensations += 1;
            } else {
                break;
            }
        }
        if let Some(root) = self.root.clone() {
            if self.n(&root)?.count == 0 {
                self.free(&root)?;
                self.root = None;
            }
        }
        Ok(())
    }
    /// Hilbert/Morton packed load. Replaces contents; leaves caller entries in
    /// their original order. Global bounds and curve indices precede old-tree free.
    pub fn bulk_load(&mut self, entries: &[BulkEntry<T, E, N>]) -> Result<(), Error> {
        self.ready()?;
        if entries.len() > self.max_entries {
            return Err(Error::ResourceLimit);
        }
        if entries.is_empty() {
            return self.clear();
        }
        let mut lo = [E::MAX; N];
        let mut hi = [E::LOWEST; N];
        for e in entries {
            for d in 0..N {
                lo[d] = lo[d].min(e.min[d]);
                hi[d] = hi[d].max(e.max[d]);
            }
        }
        let range: [f64; N] = core::array::from_fn(|d| {
            let r = hi[d].widen() as f64 - lo[d].widen() as f64;
            if r <= 0.0 {
                1.0
            } else {
                r
            }
        });
        let mut order = Vec::new();
        reserve(&mut order, entries.len())?;
        for (i, e) in entries.iter().enumerate() {
            let mut coords = [0u32; N];
            for d in 0..N {
                let center = (e.min[d].widen() as f64 + e.max[d].widen() as f64) / 2.0;
                let scaled = ((center - lo[d].widen() as f64) / range[d]) * f64::from(u32::MAX);
                // C++ truncates toward zero; (-1,0) is representable as zero.
                if !scaled.is_finite() || scaled <= -1.0 || scaled >= 4294967296.0 {
                    return Err(Error::Conversion);
                }
                coords[d] = scaled as u32;
            }
            order.push((hilbert_nd(32, &coords)?, i));
        }
        // The (curve,index) pair has a strict unique total order, so unstable
        // sort's equal-key behavior cannot change this ordering.
        order.sort_unstable();
        self.interrupted = true;
        self.remove_all_nodes(self.root.clone())?;
        self.root = None;
        self.count = entries.len();
        let mut current = Vec::new();
        reserve(&mut current, entries.len().div_ceil(M))?;
        for group in order.chunks(M) {
            let leaf = self.alloc(0)?;
            for (_, i) in group {
                let e = entries.get(*i).ok_or(Error::Invariant)?;
                self.append(&leaf, &Entry::leaf(e.min, e.max, e.data.clone()))?;
            }
            current.push(leaf);
        }
        let mut level = 1;
        while current.len() > 1 {
            let mut next = Vec::new();
            reserve(&mut next, current.len().div_ceil(M))?;
            for group in current.chunks(M) {
                let parent = self.alloc(level)?;
                for child in group {
                    let (lo, hi) = self.n(child)?.compute_enclosing_bounds()?;
                    self.append(&parent, &Entry::internal(lo, hi, child.clone()))?;
                }
                next.push(parent);
            }
            current = next;
            level += 1;
        }
        self.root = current.pop();
        self.interrupted = false;
        Ok(())
    }
    pub fn begin(&self) -> Result<Cursor<'_, T, E, N, M>, Error> {
        self.ready()?;
        Cursor::new(self, None)
    }
    pub fn end(&self) -> Cursor<'_, T, E, N, M> {
        Cursor::end(self)
    }
    pub fn overlapping(
        &self,
        min: [E; N],
        max: [E; N],
    ) -> Result<SearchRange<'_, T, E, N, M>, Error> {
        self.ready()?;
        Ok(SearchRange::new(self, min, max))
    }
    /// Reports into the caller's visitor without clearing caller outputs. The hit
    /// causing false is counted, and false propagates across every ancestor.
    pub fn search<F: FnMut(Option<&T>) -> bool>(
        &self,
        min: [E; N],
        max: [E; N],
        visitor: &mut F,
    ) -> Result<i32, Error> {
        self.ready()?;
        let mut found = 0;
        if let Some(root) = &self.root {
            self.search_impl(root, &min, &max, visitor, &mut found, 0)?;
        }
        Ok(found)
    }
    fn search_impl<F: FnMut(Option<&T>) -> bool>(
        &self,
        h: &NodeHandle,
        min: &[E; N],
        max: &[E; N],
        visitor: &mut F,
        found: &mut i32,
        depth: usize,
    ) -> Result<bool, Error> {
        if depth > 64 {
            return Err(Error::ResourceLimit);
        }
        let node = self.n(h)?;
        let mut mask = node.child_overlap_mask(min, max)?;
        while mask != 0 {
            let i = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if node.is_leaf() {
                *found = found.checked_add(1).ok_or(Error::Overflow)?;
                if !visitor(node.data(i)?) {
                    return Ok(false);
                }
            } else if !self.search_impl(&self.child(h, i)?, min, max, visitor, found, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn min_dist(&self, h: &NodeHandle, i: usize, point: &[E; N]) -> Result<i64, Error> {
        let (lo, hi) = self.n(h)?.get_child_bounds(i)?;
        let mut distance = 0;
        for d in 0..N {
            let diff = if point[d] < lo[d] {
                sub(lo[d].widen(), point[d].widen())?
            } else if point[d] > hi[d] {
                sub(point[d].widen(), hi[d].widen())?
            } else {
                continue;
            };
            distance = add(distance, mul(diff, diff)?)?;
        }
        Ok(distance)
    }
    /// Always clears output, including empty trees and nonpositive k. Best-first
    /// traversal visits bounding boxes, using the reference heap's distance-only
    /// comparator (equal distances are not sorted by item identifier).
    pub fn nearest_neighbors(
        &self,
        point: [E; N],
        k: i32,
        out: &mut Vec<(i64, Option<T>)>,
    ) -> Result<(), Error> {
        out.clear();
        self.ready()?;
        let Some(root) = &self.root else {
            return Ok(());
        };
        if k <= 0 {
            return Ok(());
        }
        let cmp = |a: &(i64, NodeHandle, usize), b: &(i64, NodeHandle, usize)| a.0 > b.0;
        let mut heap = Vec::new();
        for i in 0..self.n(root)?.count as usize {
            order::heap_push(
                &mut heap,
                (self.min_dist(root, i, &point)?, root.clone(), i),
                &cmp,
            )?;
        }
        while !heap.is_empty() && out.len() < k as usize {
            let (dist, node, i) = order::heap_pop(&mut heap, &cmp)?.ok_or(Error::Invariant)?;
            if self.n(&node)?.is_leaf() {
                push(out, (dist, self.n(&node)?.data(i)?.cloned()))?;
            } else {
                let child = self.child(&node, i)?;
                for j in 0..self.n(&child)?.count as usize {
                    order::heap_push(
                        &mut heap,
                        (self.min_dist(&child, j, &point)?, child.clone(), j),
                        &cmp,
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// Namespace corresponding to the owned pinned header.
pub mod dynamic_rtree {
    pub use crate::{Branches, BulkEntry, Cursor, DynamicRtree, Error, SearchRange};
}
