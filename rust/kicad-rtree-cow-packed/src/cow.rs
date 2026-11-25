// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    order, push, reserve, template, uint, BulkEntry, Coord, Error, NodeData, NodeHandle, RtreeNode,
};
use kicad_rtree_node::{hilbert_xy2d, AllocatorId, SlabArena};
use std::rc::{Rc, Weak};

/// Host limits bound retained slab pages, persistent chain metadata, and work.
/// Nodes counts allocation calls (including reuse); trees counts lifetime creations.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub entries: usize,
    pub trees: usize,
    pub nodes: usize,
}
#[derive(Clone, Debug)]
pub struct TreeId {
    token: Weak<()>,
    index: usize,
}
struct Tree {
    root: Option<NodeHandle>,
    count: usize,
    allocator: Option<usize>,
    chain: Option<usize>,
    interrupted: bool,
}
impl Tree {
    fn moved() -> Self {
        Self {
            root: None,
            count: 0,
            allocator: None,
            chain: None,
            interrupted: false,
        }
    }
}
struct Lease {
    id: AllocatorId,
    refs: usize,
}
struct Chain {
    allocator: Option<usize>,
    next: Option<usize>,
    refs: usize,
}
type Node<T, E, const N: usize, const M: usize> = RtreeNode<T, E, N, M>;

/// Shared address space for COW trees, analogous to the accepted SlabArena.
/// Tree IDs have explicit destruction, so no RefCell/recursive Rc destruction or
/// fallible operations are hidden in a handle's Drop. Dropping the forest ends
/// every remaining tree and allocator lifetime together. Cursors borrow it.
pub struct CowForest<T: NodeData, E: Coord, const N: usize = 2, const M: usize = 16> {
    arena: SlabArena<Node<T, E, N, M>>,
    token: Rc<()>,
    trees: Vec<Option<Tree>>,
    leases: Vec<Option<Lease>>,
    chains: Vec<Option<Chain>>,
    budget: Budget,
    allocated: usize,
}
impl<T: NodeData, E: Coord, const N: usize, const M: usize> CowForest<T, E, N, M> {
    pub fn new(budget: Budget) -> Result<Self, Error> {
        template::<N, M>()?;
        Ok(Self {
            arena: SlabArena::new(),
            token: Rc::new(()),
            trees: Vec::new(),
            leases: Vec::new(),
            chains: Vec::new(),
            budget,
            allocated: 0,
        })
    }
    fn index(&self, id: &TreeId) -> Result<usize, Error> {
        if !Weak::ptr_eq(&id.token, &Rc::downgrade(&self.token)) {
            return Err(Error::Invariant);
        }
        if self.trees.get(id.index).and_then(Option::as_ref).is_none() {
            return Err(Error::Invariant);
        }
        Ok(id.index)
    }
    fn state(&self, id: &TreeId) -> Result<&Tree, Error> {
        let t = self
            .trees
            .get(self.index(id)?)
            .and_then(Option::as_ref)
            .ok_or(Error::Invariant)?;
        if t.interrupted {
            return Err(Error::InterruptedMutation);
        }
        Ok(t)
    }
    fn add_tree(&mut self, t: Tree) -> Result<TreeId, Error> {
        let index = self.trees.len();
        push(&mut self.trees, Some(t))?;
        Ok(TreeId {
            token: Rc::downgrade(&self.token),
            index,
        })
    }
    fn capacity(&mut self) -> Result<(), Error> {
        if self.trees.len() >= self.budget.trees {
            return Err(Error::ResourceLimit);
        }
        reserve(&mut self.trees, 1)?;
        reserve(&mut self.leases, 1)?;
        reserve(&mut self.chains, 1)
    }
    fn new_lease(&mut self) -> Result<usize, Error> {
        let i = self.leases.len();
        let id = self.arena.new_allocator()?;
        push(&mut self.leases, Some(Lease { id, refs: 1 }))?;
        Ok(i)
    }
    fn lease(&self, i: usize) -> Result<&Lease, Error> {
        self.leases
            .get(i)
            .and_then(Option::as_ref)
            .ok_or(Error::Invariant)
    }
    fn lease_mut(&mut self, i: usize) -> Result<&mut Lease, Error> {
        self.leases
            .get_mut(i)
            .and_then(Option::as_mut)
            .ok_or(Error::Invariant)
    }
    fn chain_mut(&mut self, i: usize) -> Result<&mut Chain, Error> {
        self.chains
            .get_mut(i)
            .and_then(Option::as_mut)
            .ok_or(Error::Invariant)
    }
    pub fn new_tree(&mut self) -> Result<TreeId, Error> {
        self.capacity()?;
        let allocator = Some(self.new_lease()?);
        self.add_tree(Tree {
            allocator,
            ..Tree::moved()
        })
    }
    /// O(1) Clone: only the root refcount and one persistent chain cell change.
    pub fn clone_tree(&mut self, id: &TreeId) -> Result<TreeId, Error> {
        let t = self.state(id)?;
        let (root, count, parent, next) = (t.root.clone(), t.count, t.allocator, t.chain);
        self.capacity()?;
        let allocator = Some(self.new_lease()?);
        if let Some(i) = parent {
            self.lease_mut(i)?.refs = self
                .lease(i)?
                .refs
                .checked_add(1)
                .ok_or(Error::ResourceLimit)?;
        }
        if let Some(i) = next {
            let c = self.chain_mut(i)?;
            c.refs = c.refs.checked_add(1).ok_or(Error::ResourceLimit)?;
        }
        let ci = self.chains.len();
        push(
            &mut self.chains,
            Some(Chain {
                allocator: parent,
                next,
                refs: 1,
            }),
        )?;
        if let Some(h) = &root {
            self.arena.node_mut(h)?.refcount_fetch_add(1);
        }
        self.add_tree(Tree {
            root,
            count,
            allocator,
            chain: Some(ci),
            interrupted: false,
        })
    }
    pub fn len(&self, id: &TreeId) -> Result<usize, Error> {
        Ok(self.state(id)?.count)
    }
    pub fn is_empty(&self, id: &TreeId) -> Result<bool, Error> {
        Ok(self.len(id)? == 0)
    }
    pub fn root_handle(&self, id: &TreeId) -> Result<Option<NodeHandle>, Error> {
        Ok(self.state(id)?.root.clone())
    }
    /// Observe real allocator-chain accounting without exposing physical addresses.
    pub fn allocator_memory(&self, id: &TreeId) -> Result<Vec<Option<u64>>, Error> {
        let t = self.state(id)?;
        let mut out = Vec::new();
        let mut allocator = t.allocator;
        let mut chain = t.chain;
        loop {
            let memory = allocator
                .map(|a| {
                    self.arena
                        .memory_usage(&self.lease(a)?.id)
                        .map_err(Error::from)
                })
                .transpose()?;
            push(&mut out, memory)?;
            let Some(i) = chain else {
                break;
            };
            let c = self
                .chains
                .get(i)
                .and_then(Option::as_ref)
                .ok_or(Error::Invariant)?;
            allocator = c.allocator;
            chain = c.next;
        }
        Ok(out)
    }
    pub fn node(&self, h: &NodeHandle) -> Result<&Node<T, E, N, M>, Error> {
        Ok(self.arena.node(h)?)
    }
    fn nm(&mut self, h: &NodeHandle) -> Result<&mut Node<T, E, N, M>, Error> {
        Ok(self.arena.node_mut(h)?)
    }
    fn child(&self, h: &NodeHandle, i: usize) -> Result<NodeHandle, Error> {
        self.node(h)?.child(i)?.cloned().ok_or(Error::Invariant)
    }
    fn alloc(&mut self, t: &Tree, level: i32) -> Result<NodeHandle, Error> {
        if self.allocated >= self.budget.nodes {
            return Err(Error::ResourceLimit);
        }
        let id = self.lease(t.allocator.ok_or(Error::Invariant)?)?.id.clone();
        let h = self.arena.allocate(&id)?;
        self.allocated += 1;
        self.nm(&h)?.level = level;
        Ok(h)
    }
    fn free_owner(&mut self, t: &Tree, h: &NodeHandle) -> Result<(), Error> {
        let mut allocator = t.allocator;
        let mut chain = t.chain;
        loop {
            if let Some(a) = allocator {
                let id = self.lease(a)?.id.clone();
                if self.arena.owns(&id, Some(h))? {
                    self.arena.free(&id, Some(h))?;
                    return Ok(());
                }
            }
            let Some(i) = chain else {
                return Err(Error::Invariant);
            };
            let c = self
                .chains
                .get(i)
                .and_then(Option::as_ref)
                .ok_or(Error::Invariant)?;
            allocator = c.allocator;
            chain = c.next;
        }
    }
    fn release(&mut self, t: &Tree, h: Option<NodeHandle>) -> Result<(), Error> {
        let Some(h) = h else {
            return Ok(());
        };
        // Explicit postorder matches child release order and the owner's free list.
        let mut stack = Vec::new();
        push(&mut stack, (h, false))?;
        while let Some((h, visited)) = stack.pop() {
            if visited {
                self.free_owner(t, &h)?;
                continue;
            }
            if self.nm(&h)?.refcount_fetch_sub(1) != 1 {
                continue;
            }
            push(&mut stack, (h.clone(), true))?;
            if self.node(&h)?.is_internal() {
                for i in (0..self.node(&h)?.count as usize).rev() {
                    push(&mut stack, (self.child(&h, i)?, false))?;
                }
            }
        }
        Ok(())
    }
    fn writable(&mut self, t: &Tree, h: NodeHandle) -> Result<NodeHandle, Error> {
        if self.node(&h)?.refcount_load() <= 1 {
            return Ok(h);
        }
        let copy = self.alloc(t, 0)?;
        {
            let (src, dst) = self.arena.node_pair_mut(&h, &copy)?;
            dst.count = src.count;
            dst.level = src.level;
            // memcpy includes stale unused bounds, but payload copies only live slots.
            for d in 0..N * 2 {
                for i in 0..M {
                    dst.set_bound(d, i, src.bound(d, i)?)?;
                    dst.set_insert_bound(d, i, src.insert_bound(d, i)?)?;
                }
            }
            for i in 0..src.count as usize {
                if src.is_leaf() {
                    dst.set_data(i, src.data(i)?.cloned())?;
                } else {
                    dst.set_child(i, src.child(i)?.cloned())?;
                }
            }
            dst.refcount_store(1);
        }
        if self.node(&copy)?.is_internal() {
            for i in 0..self.node(&copy)?.count as usize {
                let c = self.child(&copy, i)?;
                self.nm(&c)?.refcount_fetch_add(1);
            }
        }
        self.release(t, Some(h))?;
        Ok(copy)
    }
    fn release_lease(&mut self, i: usize) -> Result<(), Error> {
        let l = self.lease_mut(i)?;
        l.refs = l.refs.checked_sub(1).ok_or(Error::Invariant)?;
        if l.refs == 0 {
            let l = self
                .leases
                .get_mut(i)
                .and_then(Option::take)
                .ok_or(Error::Invariant)?;
            self.arena.destroy_allocator(&l.id)?;
        }
        Ok(())
    }
    fn release_chain(&mut self, mut next: Option<usize>) -> Result<(), Error> {
        while let Some(i) = next {
            let c = self.chain_mut(i)?;
            c.refs = c.refs.checked_sub(1).ok_or(Error::Invariant)?;
            if c.refs != 0 {
                break;
            }
            let c = self
                .chains
                .get_mut(i)
                .and_then(Option::take)
                .ok_or(Error::Invariant)?;
            if let Some(a) = c.allocator {
                self.release_lease(a)?;
            }
            next = c.next;
        }
        Ok(())
    }
    fn dispose(&mut self, t: Tree) -> Result<(), Error> {
        self.release(&t, t.root.clone())?;
        if let Some(a) = t.allocator {
            self.release_lease(a)?;
        }
        self.release_chain(t.chain)
    }
    pub fn destroy(&mut self, id: &TreeId) -> Result<(), Error> {
        let i = self.index(id)?;
        let t = self
            .trees
            .get_mut(i)
            .and_then(Option::take)
            .ok_or(Error::Invariant)?;
        self.dispose(t)
    }
    pub fn move_assign(&mut self, dst: &TreeId, src: &TreeId) -> Result<(), Error> {
        let a = self.index(dst)?;
        let b = self.index(src)?;
        if a == b {
            return Ok(());
        }
        let t = self
            .trees
            .get_mut(a)
            .and_then(Option::take)
            .ok_or(Error::Invariant)?;
        self.dispose(t)?;
        let t = self
            .trees
            .get_mut(b)
            .and_then(Option::take)
            .ok_or(Error::Invariant)?;
        *self.trees.get_mut(b).ok_or(Error::Invariant)? = Some(Tree::moved());
        *self.trees.get_mut(a).ok_or(Error::Invariant)? = Some(t);
        Ok(())
    }
    pub fn move_construct(&mut self, src: &TreeId) -> Result<TreeId, Error> {
        let i = self.index(src)?;
        self.capacity()?;
        let t = self
            .trees
            .get_mut(i)
            .and_then(Option::take)
            .ok_or(Error::Invariant)?;
        *self.trees.get_mut(i).ok_or(Error::Invariant)? = Some(Tree::moved());
        self.add_tree(t)
    }
    fn edit<R>(
        &mut self,
        id: &TreeId,
        f: impl FnOnce(&mut Self, &mut Tree) -> Result<R, Error>,
    ) -> Result<R, Error> {
        self.state(id)?;
        let i = self.index(id)?;
        let mut t = self
            .trees
            .get_mut(i)
            .and_then(Option::take)
            .ok_or(Error::Invariant)?;
        t.interrupted = true;
        let result = f(self, &mut t);
        t.interrupted = result.is_err();
        *self.trees.get_mut(i).ok_or(Error::Invariant)? = Some(t);
        result
    }
    pub fn clear(&mut self, id: &TreeId) -> Result<(), Error> {
        // Clear also recovers arithmetic-refused mutations; all allocated nodes
        // remain in their owning slab until the last lease ends.
        let i = self.index(id)?;
        let mut t = self
            .trees
            .get_mut(i)
            .and_then(Option::take)
            .ok_or(Error::Invariant)?;
        let r = self.release(&t, t.root.clone());
        if r.is_ok() {
            t.root = None;
            t.count = 0;
            t.interrupted = false;
        }
        *self.trees.get_mut(i).ok_or(Error::Invariant)? = Some(t);
        r
    }
    fn leaf_append(&mut self, h: &NodeHandle, e: &BulkEntry<T, E, N>) -> Result<(), Error> {
        let n = self.nm(h)?;
        let i = usize::try_from(n.count).map_err(|_| Error::Invariant)?;
        n.set_child_bounds(i, &e.min, &e.max)?;
        n.set_insert_bounds(i, &e.min, &e.max)?;
        n.set_data(i, e.data.clone())?;
        n.count += 1;
        Ok(())
    }
    fn internal_append(
        &mut self,
        h: &NodeHandle,
        c: NodeHandle,
        min: [E; N],
        max: [E; N],
    ) -> Result<(), Error> {
        let n = self.nm(h)?;
        let i = n.count as usize;
        n.set_child_bounds(i, &min, &max)?;
        n.set_child(i, Some(c))?;
        n.count += 1;
        Ok(())
    }
    pub fn insert(
        &mut self,
        id: &TreeId,
        min: [E; N],
        max: [E; N],
        data: Option<T>,
    ) -> Result<(), Error> {
        if self.state(id)?.count >= self.budget.entries {
            return Err(Error::ResourceLimit);
        }
        self.edit(id, |s, t| {
            let mut h = if let Some(h) = t.root.clone() {
                s.writable(t, h)?
            } else {
                s.alloc(t, 0)?
            };
            t.root = Some(h.clone());
            let mut path = Vec::new();
            while !s.node(&h)?.is_leaf() {
                let mut best = 0;
                let mut area = i64::MAX;
                for i in 0..s.node(&h)?.count as usize {
                    let inc = s.node(&h)?.child_enlargement(i, &min, &max)?;
                    if inc < area {
                        area = inc;
                        best = i;
                    }
                }
                let child = s.writable(t, s.child(&h, best)?)?;
                s.nm(&h)?.set_child(best, Some(child.clone()))?;
                push(&mut path, (h, best))?;
                h = child;
            }
            let e = BulkEntry { min, max, data };
            if s.node(&h)?.is_full() {
                s.split(t, &h, e)?;
            } else {
                s.leaf_append(&h, &e)?;
            }
            for (parent, i) in path.into_iter().rev() {
                let c = s.child(&parent, i)?;
                let (lo, hi) = s.node(&c)?.compute_enclosing_bounds()?;
                s.nm(&parent)?.set_child_bounds(i, &lo, &hi)?;
                // The reference's decreased-count/internal-child arm is a no-op.
            }
            t.count = t.count.checked_add(1).ok_or(Error::ResourceLimit)?;
            Ok(())
        })
    }
    fn split(&mut self, t: &mut Tree, h: &NodeHandle, e: BulkEntry<T, E, N>) -> Result<(), Error> {
        let (lo, hi) = self.node(h)?.compute_enclosing_bounds()?;
        let mut axis = 0;
        let mut extent = 0;
        for d in 0..N {
            let x = hi[d]
                .max(e.max[d])
                .widen()
                .checked_sub(lo[d].min(e.min[d]).widen())
                .ok_or(Error::Overflow)?;
            // C++ subtracts in ELEMTYPE here, before widening.
            if x < E::LOWEST.widen() || x > E::MAX.widen() {
                return Err(Error::Overflow);
            }
            if x > extent {
                extent = x;
                axis = d;
            }
        }
        let mut entries = Vec::new();
        reserve(&mut entries, M.checked_add(1).ok_or(Error::ResourceLimit)?)?;
        for i in 0..self.node(h)?.count as usize {
            let n = self.node(h)?;
            let (min, max) = n.get_child_bounds(i)?;
            let (il, ih) = n.get_insert_bounds(i)?;
            let key = min[axis]
                .widen()
                .checked_add(max[axis].widen())
                .ok_or(Error::Overflow)?;
            entries.push((
                BulkEntry {
                    min,
                    max,
                    data: n.data(i)?.cloned(),
                },
                il,
                ih,
                key,
            ));
        }
        let key = e.min[axis]
            .widen()
            .checked_add(e.max[axis].widen())
            .ok_or(Error::Overflow)?;
        entries.push((e.clone(), e.min, e.max, key));
        order::sort(&mut entries, |a, b| a.3 < b.3)?;
        let split = entries.len() / 2;
        self.nm(h)?.count = 0;
        for (e, il, ih, _) in entries.iter().take(split) {
            self.leaf_append(h, e)?;
            let i = self.node(h)?.count as usize - 1;
            self.nm(h)?.set_insert_bounds(i, il, ih)?;
        }
        let sibling = self.alloc(t, 0)?;
        for (e, il, ih, _) in entries.iter().skip(split) {
            self.leaf_append(&sibling, e)?;
            let i = self.node(&sibling)?.count as usize - 1;
            self.nm(&sibling)?.set_insert_bounds(i, il, ih)?;
        }
        let (smin, smax) = self.node(&sibling)?.compute_enclosing_bounds()?;
        let old = t.root.clone().ok_or(Error::Invariant)?;
        let same = old == *h;
        let level = if same {
            1
        } else {
            self.node(&old)?
                .level
                .checked_add(1)
                .ok_or(Error::Overflow)?
        };
        let root = self.alloc(t, level)?;
        let (mut rmin, mut rmax) = self.node(&old)?.compute_enclosing_bounds()?;
        if !same {
            for d in 0..N {
                rmin[d] = rmin[d].min(smin[d]);
                rmax[d] = rmax[d].max(smax[d]);
            }
        }
        self.internal_append(&root, old, rmin, rmax)?;
        self.internal_append(&root, sibling, smin, smax)?;
        t.root = Some(root);
        Ok(())
    }
    pub fn remove(
        &mut self,
        id: &TreeId,
        min: [E; N],
        max: [E; N],
        data: Option<T>,
    ) -> Result<bool, Error> {
        self.edit(id, |s, t| {
            let Some(h) = t.root.clone() else {
                return Ok(false);
            };
            let h = s.writable(t, h)?;
            t.root = Some(h.clone());
            let mut stack = Vec::new();
            push(&mut stack, (h, 0usize))?;
            let mut removed = false;
            while let Some((h, i)) = stack.last().cloned() {
                if s.node(&h)?.is_leaf() {
                    for j in 0..s.node(&h)?.count as usize {
                        if s.node(&h)?.data(j)?.cloned() == data
                            && s.node(&h)?.child_overlaps(j, &min, &max)?
                        {
                            s.nm(&h)?.remove_child(j as i32)?;
                            removed = true;
                            break;
                        }
                    }
                    stack.pop();
                    if removed {
                        break;
                    }
                } else if i >= s.node(&h)?.count as usize {
                    stack.pop();
                } else {
                    stack.last_mut().ok_or(Error::Invariant)?.1 += 1;
                    if s.node(&h)?.child_overlaps(i, &min, &max)? {
                        let c = s.writable(t, s.child(&h, i)?)?;
                        s.nm(&h)?.set_child(i, Some(c.clone()))?;
                        push(&mut stack, (c, 0))?;
                    }
                }
            }
            if !removed {
                return Ok(false);
            }
            while let Some((h, next)) = stack.pop() {
                let i = next.checked_sub(1).ok_or(Error::Invariant)?;
                let c = s.child(&h, i)?;
                if s.node(&c)?.count > 0 {
                    let (lo, hi) = s.node(&c)?.compute_enclosing_bounds()?;
                    s.nm(&h)?.set_child_bounds(i, &lo, &hi)?;
                } else {
                    s.release(t, Some(c))?;
                    s.nm(&h)?.remove_child(i as i32)?;
                }
            }
            t.count = t.count.checked_sub(1).ok_or(Error::Invariant)?;
            loop {
                let r = t.root.clone().ok_or(Error::Invariant)?;
                if !s.node(&r)?.is_internal() || s.node(&r)?.count != 1 {
                    break;
                }
                let c = s.child(&r, 0)?;
                s.nm(&c)?.refcount_fetch_add(1);
                t.root = Some(c);
                s.release(t, Some(r))?;
            }
            if let Some(r) = t.root.clone() {
                if s.node(&r)?.count == 0 {
                    s.release(t, Some(r))?;
                    t.root = None;
                }
            }
            Ok(true)
        })
    }
    pub fn bulk_load(
        &mut self,
        id: &TreeId,
        entries: &mut Vec<BulkEntry<T, E, N>>,
    ) -> Result<(), Error> {
        if entries.len() > self.budget.entries {
            return Err(Error::ResourceLimit);
        }
        self.clear(id)?;
        if entries.is_empty() {
            return Ok(());
        }
        // The pinned COW body reads axis 1 unconditionally (unlike p001).
        if N < 2 {
            return Err(Error::InvalidTemplate);
        }
        self.edit(id, |s, t| {
            t.count = entries.len();
            let first = entries.first().ok_or(Error::Invariant)?;
            let (mut lo, mut hi) = (first.min, first.max);
            for e in entries.iter() {
                for d in 0..N {
                    lo[d] = lo[d].min(e.min[d]);
                    hi[d] = hi[d].max(e.max[d]);
                }
            }
            let mut scale = [0.0; N];
            for d in 0..N {
                let range = hi[d].widen() as f64 - lo[d].widen() as f64;
                scale[d] = if range > 0.0 { 65535.0 / range } else { 0.0 };
            }
            let mut sorted = Vec::new();
            reserve(&mut sorted, entries.len())?;
            for (i, e) in entries.iter().enumerate() {
                let cx = (e.min[0].widen() as f64 + e.max[0].widen() as f64) * 0.5;
                let cy = (e.min[1].widen() as f64 + e.max[1].widen() as f64) * 0.5;
                let x = uint((cx - lo[0].widen() as f64) * scale[0])?;
                let y = uint((cy - lo[1].widen() as f64) * scale[1])?;
                sorted.push((hilbert_xy2d(16, x, y)?, i));
            }
            order::sort(&mut sorted, |a, b| a.0 < b.0)?;
            let mut reordered = Vec::new();
            reserve(&mut reordered, entries.len())?;
            for (_, i) in sorted {
                reordered.push(entries.get(i).ok_or(Error::Invariant)?.clone());
            }
            *entries = reordered;
            let mut level = Vec::new();
            for chunk in entries.chunks(M) {
                let leaf = s.alloc(t, 0)?;
                for e in chunk {
                    s.leaf_append(&leaf, e)?;
                }
                push(&mut level, leaf)?;
            }
            let mut depth = 1i32;
            while level.len() > 1 {
                let mut next = Vec::new();
                for chunk in level.chunks(M) {
                    let parent = s.alloc(t, depth)?;
                    for c in chunk {
                        let (lo, hi) = s.node(c)?.compute_enclosing_bounds()?;
                        s.internal_append(&parent, c.clone(), lo, hi)?;
                    }
                    push(&mut next, parent)?;
                }
                level = next;
                depth = depth.checked_add(1).ok_or(Error::Overflow)?;
            }
            t.root = level.pop();
            Ok(())
        })
    }
    pub fn search(
        &self,
        id: &TreeId,
        min: [E; N],
        max: [E; N],
        mut visitor: impl FnMut(&Option<T>) -> bool,
    ) -> Result<i32, Error> {
        let mut stack = Vec::new();
        if let Some(h) = &self.state(id)?.root {
            push(&mut stack, (h.clone(), 0usize))?;
        }
        let mut found = 0i32;
        while let Some((h, i)) = stack.pop() {
            let n = self.node(&h)?;
            if i >= n.count as usize {
                continue;
            }
            if n.is_leaf() {
                if n.child_overlaps(i, &min, &max)? {
                    found = found.checked_add(1).ok_or(Error::Overflow)?;
                    if !visitor(&n.data(i)?.cloned()) {
                        continue;
                    } // stop only this leaf
                }
                push(&mut stack, (h, i + 1))?;
            } else {
                let overlaps = n.child_overlaps(i, &min, &max)?;
                push(&mut stack, (h.clone(), i + 1))?;
                if overlaps {
                    push(&mut stack, (self.child(&h, i)?, 0))?;
                }
            }
        }
        Ok(found)
    }
    pub fn begin(&self, id: &TreeId) -> Result<CowCursor<'_, T, E, N, M>, Error> {
        let mut c = self.end();
        if let Some(h) = &self.state(id)?.root {
            if self.node(h)?.count > 0 {
                push(&mut c.stack, (h.clone(), 0))?;
                c.advance()?;
            }
        }
        Ok(c)
    }
    pub fn end(&self) -> CowCursor<'_, T, E, N, M> {
        CowCursor {
            forest: self,
            stack: Vec::new(),
            current: None,
            at_end: true,
        }
    }
}
#[derive(Clone)]
pub struct CowCursor<'a, T: NodeData, E: Coord, const N: usize, const M: usize> {
    forest: &'a CowForest<T, E, N, M>,
    stack: Vec<(NodeHandle, usize)>,
    current: Option<T>,
    at_end: bool,
}
impl<T: NodeData, E: Coord, const N: usize, const M: usize> CowCursor<'_, T, E, N, M> {
    pub fn current(&self) -> &Option<T> {
        &self.current
    }
    pub fn is_end(&self) -> bool {
        self.at_end
    }
    pub fn advance(&mut self) -> Result<(), Error> {
        while let Some((h, i)) = self.stack.last().cloned() {
            let n = self.forest.node(&h)?;
            if i >= n.count as usize {
                self.stack.pop();
                continue;
            }
            self.stack.last_mut().ok_or(Error::Invariant)?.1 += 1;
            if n.is_leaf() {
                self.current = n.data(i)?.cloned();
                self.at_end = false;
                return Ok(());
            }
            push(&mut self.stack, (self.forest.child(&h, i)?, 0))?;
        }
        self.at_end = true;
        Ok(())
    }
}
impl<T: NodeData, E: Coord, const N: usize, const M: usize> PartialEq
    for CowCursor<'_, T, E, N, M>
{
    fn eq(&self, other: &Self) -> bool {
        self.at_end == other.at_end
    }
}
impl<T: NodeData, E: Coord, const N: usize, const M: usize> Eq for CowCursor<'_, T, E, N, M> {}
