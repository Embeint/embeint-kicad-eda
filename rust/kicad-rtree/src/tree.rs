// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{add, float_i64, mul, order, sub, Coord, Error, NodeData, Result};
use kicad_rtree_node::{hilbert_nd, AllocatorId, NodeHandle, RtreeNode, SlabArena};

type Node<T, E, const D: usize, const F: usize> = RtreeNode<T, E, D, F>;
type Bounds<E, const D: usize> = ([E; D], [E; D]);
#[derive(Clone, Debug, PartialEq)]
pub struct BulkEntry<T, E, const D: usize> {
    pub min: [E; D],
    pub max: [E; D],
    pub data: T,
}
#[derive(Clone)]
struct Entry<T, E, const D: usize> {
    min: [E; D],
    max: [E; D],
    insert_min: [E; D],
    insert_max: [E; D],
    data: Option<T>,
    child: Option<NodeHandle>,
}
impl<T, E: Copy, const D: usize> Entry<T, E, D> {
    fn leaf(min: [E; D], max: [E; D], data: T) -> Self {
        Self {
            min,
            max,
            insert_min: min,
            insert_max: max,
            data: Some(data),
            child: None,
        }
    }
    fn internal(min: [E; D], max: [E; D], child: NodeHandle) -> Self {
        Self {
            min,
            max,
            insert_min: min,
            insert_max: max,
            data: None,
            child: Some(child),
        }
    }
}
/// Diagnostic shape counts, never pointer values. Useful for structural tests.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    pub height: i32,
    pub nodes: usize,
    pub leaves: usize,
}

/// Host resource policy. Limits apply to work/storage, never coordinate validity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceBudget {
    pub max_items: usize,
    /// Active nodes. Slabs round storage up to 256 nodes; freed nodes are reused.
    pub max_nodes: usize,
}
impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_items: 1_000_000,
            max_nodes: 65_536,
        }
    }
}

/// One owning dynamic tree. The arena and its allocator move together. Rust
/// borrowing prevents mutation while an iterator/range is alive.
pub struct DynamicRtree<T: NodeData, E: Coord, const D: usize = 2, const F: usize = 16> {
    pub(crate) arena: SlabArena<Node<T, E, D, F>>,
    allocator: AllocatorId,
    pub(crate) root: Option<NodeHandle>,
    count: usize,
    budget: ResourceBudget,
    active_nodes: usize,
    failed: bool,
}
impl<T: NodeData, E: Coord, const D: usize, const F: usize> DynamicRtree<T, E, D, F> {
    /// Default budget is one million items. A different budget is a host resource
    /// policy, not a restriction of the mathematical valid domain.
    pub fn new() -> Result<Self> {
        Self::with_resources(ResourceBudget::default())
    }
    pub fn with_limit(limit: usize) -> Result<Self> {
        Self::with_resources(ResourceBudget {
            max_items: limit,
            ..ResourceBudget::default()
        })
    }
    pub fn with_resources(budget: ResourceBudget) -> Result<Self> {
        // 2D's accepted SIMD helper requires multiples of four, <=31. Other
        // dimensions may use any fanout >=3 up to31; MINNODES must be positive.
        if D == 0 || F < 3 || F > 31 || (D == 2 && F % 4 != 0) {
            return Err(Error::UnsupportedTemplate);
        }
        if budget.max_items > i32::MAX as usize || budget.max_nodes > i32::MAX as usize {
            return Err(Error::Resource);
        }
        let mut arena = SlabArena::new();
        let allocator = arena.new_allocator()?;
        Ok(Self {
            arena,
            allocator,
            root: None,
            count: 0,
            budget,
            active_nodes: 0,
            failed: false,
        })
    }
    pub(crate) fn ready(&self) -> Result<()> {
        if self.failed {
            Err(Error::FailedMutation)
        } else {
            Ok(())
        }
    }
    pub fn size(&self) -> Result<usize> {
        self.ready()?;
        Ok(self.count)
    }
    pub fn empty(&self) -> Result<bool> {
        Ok(self.size()? == 0)
    }
    /// Pinned Linux LP64/libstdc++ accounting, not Rust heap bytes. Uses the
    /// accepted slab's measured node/page layout and modeled free-list capacity.
    pub fn memory_usage(&self) -> Result<u64> {
        self.ready()?;
        Ok(self.arena.memory_usage(&self.allocator)?)
    }
    /// Move construction: source is empty with a fresh allocator. All handles and
    /// pages stay with the destination; no nodes are copied.
    pub fn move_out(&mut self) -> Result<Self> {
        let empty = Self::with_resources(self.budget)?;
        Ok(core::mem::replace(self, empty))
    }
    /// Move assignment destroys the previous destination storage by ownership.
    pub fn move_assign(&mut self, source: &mut Self) -> Result<()> {
        *self = source.move_out()?;
        Ok(())
    }
    pub(crate) fn node(&self, h: &NodeHandle) -> Result<&Node<T, E, D, F>> {
        Ok(self.arena.node(h)?)
    }
    fn node_mut(&mut self, h: &NodeHandle) -> Result<&mut Node<T, E, D, F>> {
        Ok(self.arena.node_mut(h)?)
    }
    fn alloc(&mut self, level: i32) -> Result<NodeHandle> {
        if self.active_nodes >= self.budget.max_nodes {
            return Err(Error::Resource);
        }
        let h = self.arena.allocate(&self.allocator)?;
        self.active_nodes += 1;
        self.node_mut(&h)?.level = level;
        Ok(h)
    }
    fn free(&mut self, h: &NodeHandle) -> Result<()> {
        self.arena.free(&self.allocator, Some(h))?;
        self.active_nodes = self.active_nodes.checked_sub(1).ok_or(Error::Invariant)?;
        Ok(())
    }
    fn ncount(&self, h: &NodeHandle) -> Result<usize> {
        usize::try_from(self.node(h)?.count).map_err(|_| Error::Invariant)
    }
    pub(crate) fn child(&self, h: &NodeHandle, i: usize) -> Result<NodeHandle> {
        self.node(h)?.child(i)?.cloned().ok_or(Error::Invariant)
    }
    pub(crate) fn data(&self, h: &NodeHandle, i: usize) -> Result<T> {
        self.node(h)?.data(i)?.cloned().ok_or(Error::Invariant)
    }
    fn bounds(&self, h: &NodeHandle) -> Result<Bounds<E, D>> {
        Ok(self.node(h)?.compute_enclosing_bounds()?)
    }
    fn entries(&self, h: &NodeHandle) -> Result<Vec<Entry<T, E, D>>> {
        let mut out = Vec::new();
        out.try_reserve(F + 1).map_err(|_| Error::Resource)?;
        let n = self.node(h)?;
        for i in 0..self.ncount(h)? {
            let (min, max) = n.get_child_bounds(i)?;
            out.push(if n.is_leaf() {
                let (insert_min, insert_max) = n.get_insert_bounds(i)?;
                Entry {
                    min,
                    max,
                    insert_min,
                    insert_max,
                    data: Some(self.data(h, i)?),
                    child: None,
                }
            } else {
                Entry::internal(min, max, self.child(h, i)?)
            });
        }
        Ok(out)
    }
    fn append(&mut self, h: &NodeHandle, e: &Entry<T, E, D>) -> Result<()> {
        let slot = self.ncount(h)?;
        let n = self.node_mut(h)?;
        n.set_child_bounds(slot, &e.min, &e.max)?;
        if n.is_leaf() {
            n.set_insert_bounds(slot, &e.insert_min, &e.insert_max)?;
            n.set_data(slot, e.data.clone())?;
        } else {
            n.set_child(slot, e.child.clone())?;
        }
        n.count = n.count.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    fn rebuild(&mut self, h: &NodeHandle, entries: &[Entry<T, E, D>]) -> Result<()> {
        self.node_mut(h)?.count = 0;
        for e in entries {
            self.append(h, e)?;
        }
        Ok(())
    }
    pub fn insert(&mut self, min: [E; D], max: [E; D], data: T) -> Result<()> {
        self.ready()?;
        if self.count >= self.budget.max_items {
            return Err(Error::Resource);
        }
        let result = (|| {
            if self.root.is_none() {
                self.root = Some(self.alloc(0)?);
            }
            self.insert_impl(Entry::leaf(min, max, data), &mut 0)?;
            self.count += 1;
            Ok(())
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn insert_impl(&mut self, e: Entry<T, E, D>, levels: &mut u32) -> Result<()> {
        let (h, mut path) = self.choose(&e, None)?;
        if !self.node(&h)?.is_full() {
            self.append(&h, &e)?;
            self.adjust(&path, &h)
        } else {
            let level = self.node(&h)?.level;
            if level < 32 && (*levels & (1_u32 << level)) == 0 {
                *levels |= 1_u32 << level;
                self.forced_reinsert(&h, e, &mut path, levels)
            } else {
                self.split(&h, e, &mut path, levels, false)
            }
        }
    }
    fn overlap(&self, h: &NodeHandle, i: usize, extra: Option<&Entry<T, E, D>>) -> Result<i64> {
        let n = self.node(h)?;
        let (mut min, mut max) = n.get_child_bounds(i)?;
        if let Some(e) = extra {
            for d in 0..D {
                min[d] = min[d].min(e.min[d]);
                max[d] = max[d].max(e.max[d]);
            }
        }
        let mut total = 0;
        for j in 0..self.ncount(h)? {
            if i != j {
                total = add(total, n.child_overlap_area(j, &min, &max)?)?;
            }
        }
        Ok(total)
    }
    fn choose(
        &self,
        e: &Entry<T, E, D>,
        target: Option<i32>,
    ) -> Result<(NodeHandle, Vec<NodeHandle>)> {
        let mut h = self.root.clone().ok_or(Error::Invariant)?;
        let mut path = Vec::new();
        while self.node(&h)?.level > target.unwrap_or(0) {
            path.try_reserve(1).map_err(|_| Error::Resource)?;
            path.push(h.clone());
            let n = self.node(&h)?;
            let use_overlap = target.is_none() && n.level == 1;
            let mut best = 0;
            let mut score = (i64::MAX, i64::MAX, i64::MAX);
            for i in 0..self.ncount(&h)? {
                // Preserve reference evaluation order: overlap before enlargement.
                let ov = if use_overlap {
                    let before = self.overlap(&h, i, None)?;
                    sub(self.overlap(&h, i, Some(e))?, before)?
                } else {
                    0
                };
                let inc = n.child_enlargement(i, &e.min, &e.max)?;
                let area = n.child_area(i)?;
                let candidate = (ov, inc, area);
                if candidate < score {
                    best = i;
                    score = candidate;
                }
            }
            h = self.child(&h, best)?;
        }
        Ok((h, path))
    }
    fn slot(&self, parent: &NodeHandle, child: &NodeHandle) -> Result<Option<usize>> {
        for i in 0..self.ncount(parent)? {
            if self.child(parent, i)? == *child {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
    fn update(&mut self, parent: &NodeHandle, child: &NodeHandle) -> Result<()> {
        if let Some(i) = self.slot(parent, child)? {
            let (min, max) = self.bounds(child)?;
            self.node_mut(parent)?.set_child_bounds(i, &min, &max)?;
        }
        Ok(())
    }
    fn adjust(&mut self, path: &[NodeHandle], bottom: &NodeHandle) -> Result<()> {
        let mut child = bottom.clone();
        for parent in path.iter().rev() {
            self.update(parent, &child)?;
            child = parent.clone();
        }
        Ok(())
    }
    fn forced_reinsert(
        &mut self,
        h: &NodeHandle,
        e: Entry<T, E, D>,
        path: &mut Vec<NodeHandle>,
        levels: &mut u32,
    ) -> Result<()> {
        let (min, max) = self.bounds(h)?;
        let center: [f64; D] =
            core::array::from_fn(|d| (min[d].widen() as f64 + max[d].widen() as f64) / 2.0);
        let mut entries = self.entries(h)?;
        entries.push(e.clone());
        let mut ranked = Vec::new();
        ranked
            .try_reserve(entries.len())
            .map_err(|_| Error::Resource)?;
        for e in entries {
            let mut dist = 0;
            for (d, c) in center.iter().enumerate() {
                let entry_center = (e.min[d].widen() as f64 + e.max[d].widen() as f64) / 2.0;
                let diff = entry_center - c;
                dist = add(dist, float_i64(diff * diff)?)?;
            }
            ranked.push((dist, e));
        }
        order::sort(&mut ranked, |a, b| b.0.cmp(&a.0))?;
        let reinsert = (F * 3 / 10).min(ranked.len() - Self::min_nodes());
        if reinsert == 0 {
            return self.split(h, e, path, levels, false);
        }
        self.node_mut(h)?.count = 0;
        for (_, e) in ranked.iter().skip(reinsert) {
            self.append(h, e)?;
        }
        self.adjust(path, h)?;
        for (_, e) in ranked.into_iter().take(reinsert) {
            if self.node(h)?.is_leaf() {
                self.insert_impl(
                    Entry::leaf(e.insert_min, e.insert_max, e.data.ok_or(Error::Invariant)?),
                    levels,
                )?;
            } else {
                self.reinsert_node(e, self.node(h)?.level - 1, levels)?;
            }
        }
        Ok(())
    }
    fn reinsert_node(&mut self, e: Entry<T, E, D>, level: i32, levels: &mut u32) -> Result<()> {
        let (h, mut path) = self.choose(&e, Some(level + 1))?;
        if !self.node(&h)?.is_full() {
            self.append(&h, &e)?;
            self.adjust(&path, &h)
        } else {
            self.split(&h, e, &mut path, levels, true)
        }
    }
    fn min_nodes() -> usize {
        F * 2 / 5
    }
    fn group(entries: &[Entry<T, E, D>]) -> Bounds<E, D> {
        let (mut min, mut max) = ([E::MAX; D], [E::LOWEST; D]);
        for e in entries {
            for d in 0..D {
                min[d] = min[d].min(e.min[d]);
                max[d] = max[d].max(e.max[d]);
            }
        }
        (min, max)
    }
    fn area(bounds: Bounds<E, D>) -> Result<i64> {
        let mut a = 1;
        for d in 0..D {
            a = mul(a, sub(bounds.1[d].widen(), bounds.0[d].widen())?)?;
        }
        Ok(a)
    }
    fn perimeters(entries: &[Entry<T, E, D>]) -> Result<i64> {
        let mut sum = 0;
        for k in Self::min_nodes()..=entries.len() - Self::min_nodes() {
            let (a, b) = entries.split_at_checked(k).ok_or(Error::Invariant)?;
            for group in [a, b] {
                let (min, max) = Self::group(group);
                let mut perimeter = 0;
                for d in 0..D {
                    perimeter = add(perimeter, sub(max[d].widen(), min[d].widen())?)?;
                }
                sum = add(sum, mul(2, perimeter)?)?;
            }
        }
        Ok(sum)
    }
    fn split_overlap(entries: &[Entry<T, E, D>], k: usize) -> Result<i64> {
        let (a, b) = entries.split_at_checked(k).ok_or(Error::Invariant)?;
        let (amin, amax) = Self::group(a);
        let (bmin, bmax) = Self::group(b);
        let mut overlap = 1;
        for d in 0..D {
            let lo = amin[d].max(bmin[d]);
            let hi = amax[d].min(bmax[d]);
            if lo >= hi {
                return Ok(0);
            }
            overlap = mul(overlap, sub(hi.widen(), lo.widen())?)?;
        }
        Ok(overlap)
    }
    fn split_index(entries: &[Entry<T, E, D>]) -> Result<usize> {
        let mut best = Self::min_nodes();
        let mut score = (i64::MAX, i64::MAX);
        for k in Self::min_nodes()..=entries.len() - Self::min_nodes() {
            let overlap = Self::split_overlap(entries, k)?;
            let (a, b) = entries.split_at_checked(k).ok_or(Error::Invariant)?;
            let area = add(Self::area(Self::group(a))?, Self::area(Self::group(b))?)?;
            if (overlap, area) < score {
                best = k;
                score = (overlap, area);
            }
        }
        Ok(best)
    }
    fn split(
        &mut self,
        h: &NodeHandle,
        e: Entry<T, E, D>,
        path: &mut Vec<NodeHandle>,
        _levels: &mut u32,
        internal: bool,
    ) -> Result<()> {
        let mut h = h.clone();
        let mut e = e;
        let mut internal = internal;
        loop {
            let mut entries = self.entries(&h)?;
            entries.push(e);
            let mut best_axis = 0;
            let mut best_perimeter = i64::MAX;
            for axis in 0..D {
                order::sort(&mut entries, |a, b| {
                    if internal {
                        a.min[axis].cmp(&b.min[axis])
                    } else {
                        (a.min[axis], a.max[axis]).cmp(&(b.min[axis], b.max[axis]))
                    }
                })?;
                let mut perimeter = Self::perimeters(&entries)?;
                if !internal {
                    order::sort(&mut entries, |a, b| {
                        (a.max[axis], a.min[axis]).cmp(&(b.max[axis], b.min[axis]))
                    })?;
                    perimeter = add(perimeter, Self::perimeters(&entries)?)?;
                }
                if perimeter < best_perimeter {
                    best_axis = axis;
                    best_perimeter = perimeter;
                }
            }
            order::sort(&mut entries, |a, b| {
                if internal {
                    a.min[best_axis].cmp(&b.min[best_axis])
                } else {
                    (a.min[best_axis], a.max[best_axis]).cmp(&(b.min[best_axis], b.max[best_axis]))
                }
            })?;
            let mut index = Self::split_index(&entries)?;
            if !internal {
                let mut by_max = entries.clone();
                order::sort(&mut by_max, |a, b| {
                    (a.max[best_axis], a.min[best_axis]).cmp(&(b.max[best_axis], b.min[best_axis]))
                })?;
                let max_index = Self::split_index(&by_max)?;
                let overlap_min = Self::split_overlap(&entries, index)?;
                let overlap_max = Self::split_overlap(&by_max, max_index)?;
                if overlap_max < overlap_min {
                    entries = by_max;
                    index = max_index;
                }
            }
            let sibling = self.alloc(self.node(&h)?.level)?;
            let (a, b) = entries.split_at_checked(index).ok_or(Error::Invariant)?;
            self.rebuild(&h, a)?;
            self.rebuild(&sibling, b)?;
            let (min, max) = self.bounds(&sibling)?;
            let sibling_entry = Entry::internal(min, max, sibling);
            if let Some(parent) = path.pop() {
                self.update(&parent, &h)?;
                if !self.node(&parent)?.is_full() {
                    self.append(&parent, &sibling_entry)?;
                    self.adjust(path, &parent)?;
                } else {
                    h = parent;
                    e = sibling_entry;
                    internal = true;
                    continue;
                }
            } else {
                let root = self.root.as_ref().ok_or(Error::Invariant)?;
                let new_root = self.alloc(
                    self.node(root)?
                        .level
                        .checked_add(1)
                        .ok_or(Error::Overflow)?,
                )?;
                let (min, max) = self.bounds(&h)?;
                self.append(&new_root, &Entry::internal(min, max, h.clone()))?;
                self.append(&new_root, &sibling_entry)?;
                self.root = Some(new_root);
            }
            return Ok(());
        }
    }
    /// Hilbert packed bulk loading. The input slice is read, never reordered.
    pub fn bulk_load(&mut self, entries: &[BulkEntry<T, E, D>]) -> Result<()> {
        self.ready()?;
        if entries.len() > self.budget.max_items {
            return Err(Error::Resource);
        }
        if entries.is_empty() {
            return self.remove_all();
        }
        // All normalization and sort allocation precede discarding the old tree.
        let (mut min, mut max) = ([E::MAX; D], [E::LOWEST; D]);
        for e in entries {
            for d in 0..D {
                min[d] = min[d].min(e.min[d]);
                max[d] = max[d].max(e.max[d]);
            }
        }
        let range: [f64; D] = core::array::from_fn(|d| {
            let r = max[d].widen() as f64 - min[d].widen() as f64;
            if r <= 0.0 {
                1.0
            } else {
                r
            }
        });
        let mut sorted = Vec::new();
        sorted
            .try_reserve(entries.len())
            .map_err(|_| Error::Resource)?;
        for (i, e) in entries.iter().enumerate() {
            let mut coords = [0; D];
            for d in 0..D {
                let center = (e.min[d].widen() as f64 + e.max[d].widen() as f64) / 2.0;
                let v = ((center - min[d].widen() as f64) / range[d]) * u32::MAX as f64;
                if !v.is_finite() || v.trunc() < 0.0 || v.trunc() >= 4294967296.0 {
                    return Err(Error::FloatConversion);
                }
                coords[d] = v as u32;
            }
            sorted.push((hilbert_nd(32, &coords)?, i));
        }
        order::sort(&mut sorted, |a, b| a.cmp(b))?;
        let result = (|| {
            self.remove_all()?;
            self.count = entries.len();
            let mut current = Vec::new();
            current
                .try_reserve(entries.len().div_ceil(F))
                .map_err(|_| Error::Resource)?;
            for chunk in sorted.chunks(F) {
                let h = self.alloc(0)?;
                for &(_, i) in chunk {
                    let e = entries.get(i).ok_or(Error::Invariant)?;
                    self.append(&h, &Entry::leaf(e.min, e.max, e.data.clone()))?;
                }
                current.push(h);
            }
            let mut level = 1;
            while current.len() > 1 {
                let mut next = Vec::new();
                next.try_reserve(current.len().div_ceil(F))
                    .map_err(|_| Error::Resource)?;
                for chunk in current.chunks(F) {
                    let h = self.alloc(level)?;
                    for child in chunk {
                        let (min, max) = self.bounds(child)?;
                        self.append(&h, &Entry::internal(min, max, child.clone()))?;
                    }
                    next.push(h);
                }
                current = next;
                level += 1;
            }
            self.root = current.pop();
            Ok(())
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn remove_impl(
        &mut self,
        h: &NodeHandle,
        min: &[E; D],
        max: &[E; D],
        data: &T,
        orphans: &mut Vec<NodeHandle>,
    ) -> Result<bool> {
        let mut stack = Vec::new();
        stack.try_reserve(1).map_err(|_| Error::Resource)?;
        stack.push((h.clone(), 0_usize));
        while let Some((node, i)) = stack.last().cloned() {
            if i >= self.ncount(&node)? {
                stack.pop();
                continue;
            }
            stack.last_mut().ok_or(Error::Invariant)?.1 += 1;
            if self.node(&node)?.is_leaf() {
                if self.node(&node)?.data(i)? != Some(data)
                    || !self.node(&node)?.child_overlaps(i, min, max)?
                {
                    continue;
                }
                self.node_mut(&node)?.remove_child(i as i32)?;
                stack.pop();
                let mut child = node;
                for (parent, next) in stack.iter().rev() {
                    let slot = next.checked_sub(1).ok_or(Error::Invariant)?;
                    let count = self.ncount(&child)?;
                    if count > 0 {
                        let (cmin, cmax) = self.bounds(&child)?;
                        self.node_mut(parent)?
                            .set_child_bounds(slot, &cmin, &cmax)?;
                        // The reference exempts a child when its PARENT is root.
                        if count < Self::min_nodes() && Some(parent) != self.root.as_ref() {
                            orphans.try_reserve(1).map_err(|_| Error::Resource)?;
                            orphans.push(child.clone());
                            self.node_mut(parent)?.remove_child(slot as i32)?;
                        }
                    } else {
                        self.free(&child)?;
                        self.node_mut(parent)?.remove_child(slot as i32)?;
                    }
                    child = parent.clone();
                }
                return Ok(true);
            }
            if self.node(&node)?.child_overlaps(i, min, max)? {
                let child = self.child(&node, i)?;
                stack.try_reserve(1).map_err(|_| Error::Resource)?;
                stack.push((child, 0));
            }
        }
        Ok(false)
    }
    pub fn remove(&mut self, min: [E; D], max: [E; D], data: &T) -> Result<bool> {
        self.ready()?;
        let Some(root) = self.root.clone() else {
            return Ok(false);
        };
        let result = (|| {
            let mut orphans = Vec::new();
            let found = self.remove_impl(&root, &min, &max, data, &mut orphans)?
                || self.remove_impl(&root, &[E::LOWEST; D], &[E::MAX; D], data, &mut orphans)?;
            if !found {
                return Ok(false);
            }
            self.count -= 1;
            for orphan in orphans {
                let leaf = self.node(&orphan)?.is_leaf();
                let level = self.node(&orphan)?.level;
                for e in self.entries(&orphan)? {
                    if leaf {
                        self.insert_impl(
                            Entry::leaf(
                                e.insert_min,
                                e.insert_max,
                                e.data.ok_or(Error::Invariant)?,
                            ),
                            &mut 0,
                        )?;
                    } else {
                        self.reinsert_node(e, level - 1, &mut 0)?;
                    }
                }
                self.free(&orphan)?;
            }
            while let Some(root) = self.root.clone() {
                if self.node(&root)?.is_internal() && self.ncount(&root)? == 1 {
                    self.root = Some(self.child(&root, 0)?);
                    self.free(&root)?;
                } else {
                    break;
                }
            }
            if let Some(root) = self.root.clone() {
                if self.ncount(&root)? == 0 {
                    self.free(&root)?;
                    self.root = None;
                }
            }
            Ok(true)
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn remove_nodes(&mut self, h: &NodeHandle) -> Result<()> {
        let mut stack = Vec::new();
        let mut postorder = Vec::new();
        stack
            .try_reserve(self.active_nodes)
            .map_err(|_| Error::Resource)?;
        postorder
            .try_reserve(self.active_nodes)
            .map_err(|_| Error::Resource)?;
        stack.push((h.clone(), false));
        while let Some((h, visited)) = stack.pop() {
            if visited {
                postorder.push(h);
                continue;
            }
            stack.push((h.clone(), true));
            if self.node(&h)?.is_internal() {
                for i in (0..self.ncount(&h)?).rev() {
                    stack.push((self.child(&h, i)?, false));
                }
            }
        }
        for h in postorder {
            self.free(&h)?;
        }
        Ok(())
    }
    /// Releases active nodes postorder, retaining slab pages and free-list capacity.
    /// After an undefined-domain mutation refusal, drops the arena instead, because
    /// disconnected partial nodes cannot be traversed as a successful tree.
    pub fn remove_all(&mut self) -> Result<()> {
        if self.failed {
            let replacement = Self::with_resources(self.budget)?;
            *self = replacement;
            return Ok(());
        }
        if let Some(root) = self.root.clone() {
            self.remove_nodes(&root)?;
        }
        self.root = None;
        self.count = 0;
        Ok(())
    }
    pub fn structure(&self) -> Result<Structure> {
        self.ready()?;
        let mut s = Structure::default();
        if let Some(root) = self.root.as_ref() {
            s.height = self.node(root)?.level + 1;
            self.shape(root, &mut s)?;
        }
        Ok(s)
    }
    fn shape(&self, h: &NodeHandle, s: &mut Structure) -> Result<()> {
        let mut stack = Vec::new();
        stack
            .try_reserve(self.active_nodes)
            .map_err(|_| Error::Resource)?;
        stack.push(h.clone());
        while let Some(h) = stack.pop() {
            s.nodes += 1;
            if self.node(&h)?.is_leaf() {
                s.leaves += 1;
            } else {
                for i in (0..self.ncount(&h)?).rev() {
                    stack.push(self.child(&h, i)?);
                }
            }
        }
        Ok(())
    }
}
