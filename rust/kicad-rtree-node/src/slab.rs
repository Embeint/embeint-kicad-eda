// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIRTREE::SLAB_ALLOCATOR` with safe, address-free node identity.
//!
//! The reference hands out `NODE*` pointers into 256-node pages and recognises its
//! own nodes by comparing addresses with each page's node range.  Here each page
//! owns an `Rc<PageToken>`; a [`NodeHandle`] holds a `Weak` reference to that token
//! and the slot offset.  Ownership is decided by token identity, never by numeric
//! addresses, and no address reaches any output.  A live `Weak` keeps the token's
//! allocation reserved, so a handle can never be mistaken for a later page.
//!
//! Node storage lives in the allocator, so nodes are read and written through it
//! ([`SlabAllocator::node`], [`SlabAllocator::node_mut`]).  Pages, the free list and
//! the libstdc++ free-list capacity move together exactly as `m_pages` and
//! `m_freeList` do.

use crate::layout::{self, ReferenceLayout, POINTER_SIZE};
use crate::RtreeError;
use core::fmt;
use std::rc::{Rc, Weak};

/// `SLAB_ALLOCATOR::NODES_PER_PAGE`.
pub const NODES_PER_PAGE: usize = 256;
const WORDS: usize = NODES_PER_PAGE / 64;

/// A node type the allocator can construct in place.
pub trait SlabNode {
    /// `new( node ) NODE()`.
    fn construct() -> Self;
    /// `sizeof( NODE )` and `alignof( NODE )` in the pinned Linux x86-64 reference.
    fn reference_layout() -> Result<ReferenceLayout, RtreeError>;
}

/// Identity of one page; never compared by value.
#[derive(Debug)]
struct PageToken;

/// An opaque `NODE*`: the owning page's identity and the slot offset in it.
///
/// Handles compare equal exactly when they name the same slot of the same page,
/// independent of which allocator currently holds that page.
#[derive(Clone)]
pub struct NodeHandle {
    page: Weak<PageToken>,
    slot: u16,
}

impl PartialEq for NodeHandle {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.page, &other.page) && self.slot == other.slot
    }
}

impl Eq for NodeHandle {}

impl fmt::Debug for NodeHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Deliberately omits the token address.
        write!(formatter, "NodeHandle(slot {})", self.slot)
    }
}

/// `SLAB_ALLOCATOR::PAGE`: 256 constructed nodes and the `used` bitset.
struct Page<N> {
    token: Rc<PageToken>,
    nodes: Vec<N>,
    used: [u64; WORDS],
}

impl<N: SlabNode> Page<N> {
    /// `PAGE() : used() {}`: every node is default-constructed, none is used.
    fn new() -> Self {
        Self {
            token: Rc::new(PageToken),
            nodes: (0..NODES_PER_PAGE).map(|_| N::construct()).collect(),
            used: [0; WORDS],
        }
    }
}

impl<N> Page<N> {
    fn is_used(&self, offset: usize) -> bool {
        self.used
            .get(offset / 64)
            .is_some_and(|word| word >> (offset % 64) & 1 == 1)
    }

    fn set(&mut self, offset: usize, value: bool) {
        if let Some(word) = self.used.get_mut(offset / 64) {
            let bit = 1_u64 << (offset % 64);
            if value {
                *word |= bit;
            } else {
                *word &= !bit;
            }
        }
    }

    fn used_count(&self) -> usize {
        self.used
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    fn handle(&self, offset: usize) -> Result<NodeHandle, RtreeError> {
        Ok(NodeHandle {
            page: Rc::downgrade(&self.token),
            slot: u16::try_from(offset).map_err(|_| RtreeError::SlotOutOfRange)?,
        })
    }
}

/// `KIRTREE::SLAB_ALLOCATOR<NODE>`.  Not `Clone`, like the reference.
pub struct SlabAllocator<N> {
    pages: Vec<Page<N>>,
    free_list: Vec<NodeHandle>,
    /// `m_freeList.capacity()` under libstdc++'s growth policy (see
    /// [`Self::free_list_capacity`]); Rust's own `Vec` capacity is not used.
    free_list_capacity: u64,
}

impl<N> Default for SlabAllocator<N> {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            free_list: Vec::new(),
            free_list_capacity: 0,
        }
    }
}

impl<N> fmt::Debug for SlabAllocator<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SlabAllocator")
            .field("pages", &self.pages.len())
            .field("free_list", &self.free_list)
            .field("free_list_capacity", &self.free_list_capacity)
            .finish()
    }
}

impl<N: SlabNode> SlabAllocator<N> {
    /// `SLAB_ALLOCATOR() = default`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `SLAB_ALLOCATOR( SLAB_ALLOCATOR&& )`: the result takes the pages, the free
    /// list and its capacity; `self` is left with none (libstdc++ moves leave the
    /// source vectors empty with zero capacity).  Existing handles follow their
    /// pages to the result.
    pub fn take(&mut self) -> Self {
        core::mem::take(self)
    }

    /// `operator=( SLAB_ALLOCATOR&& )`: this allocator's previous pages are
    /// destroyed, then it takes `other`'s state as [`Self::take`] does.  The
    /// reference's `this != &aOther` guard has no Rust counterpart: a self move
    /// cannot be expressed through two exclusive borrows.
    pub fn move_assign(&mut self, other: &mut Self) {
        *self = other.take();
    }

    /// `Allocate()`.
    ///
    /// Reuses the most recently freed node first (re-marking it used), otherwise the
    /// first unused slot of the first page that is not full, otherwise slot 0 of a
    /// new page.  The returned node is freshly constructed in every case.
    pub fn allocate(&mut self) -> Result<NodeHandle, RtreeError> {
        if let Some(node) = self.free_list.last().cloned() {
            // Free refuses nodes outside this allocator's pages and the free list
            // moves only together with them, so the owner is always found.
            let (page, offset) = self
                .find_owning_page(&node)
                .ok_or(RtreeError::ForeignNode)?;
            self.free_list.pop();
            self.construct(page, offset)?;
            return Ok(node);
        }

        for (index, page) in self.pages.iter().enumerate() {
            if page.used_count() < NODES_PER_PAGE {
                if let Some(offset) = (0..NODES_PER_PAGE).find(|offset| !page.is_used(*offset)) {
                    let node = page.handle(offset)?;
                    self.construct(index, offset)?;
                    return Ok(node);
                }
            }
        }

        // All pages full, allocate a new one
        let page = Page::new();
        let node = page.handle(0)?;
        self.pages.push(page);
        self.construct(self.pages.len() - 1, 0)?;
        Ok(node)
    }

    fn construct(&mut self, page: usize, offset: usize) -> Result<(), RtreeError> {
        let page = self.pages.get_mut(page).ok_or(RtreeError::ForeignNode)?;
        page.set(offset, true);
        *page
            .nodes
            .get_mut(offset)
            .ok_or(RtreeError::SlotOutOfRange)? = N::construct();
        Ok(())
    }

    /// `Free( aNode )`: a null node is ignored; otherwise its slot is marked unused
    /// and the node is appended to the free list.
    ///
    /// Two reference branches are refused instead of reproduced: a node this
    /// allocator does not own ([`RtreeError::ForeignNode`]; the reference would
    /// destroy it and later re-construct it at the foreign address), and a node
    /// already freed ([`RtreeError::NodeNotLive`]; a second destructor call on an
    /// ended lifetime is undefined).
    pub fn free(&mut self, node: Option<&NodeHandle>) -> Result<(), RtreeError> {
        let Some(node) = node else {
            return Ok(());
        };
        let (index, offset) = self.find_owning_page(node).ok_or(RtreeError::ForeignNode)?;
        let page = self.pages.get_mut(index).ok_or(RtreeError::ForeignNode)?;
        if !page.is_used(offset) {
            return Err(RtreeError::NodeNotLive);
        }
        // Clear the used bit so the page-scan fallback path stays consistent.
        // ~NODE() is trivial; the stale node is unreachable until re-constructed.
        page.set(offset, false);

        // std::vector::push_back grows a full vector to size + max( size, 1 ).
        let size = u64::try_from(self.free_list.len()).map_err(|_| RtreeError::LayoutOverflow)?;
        if size == self.free_list_capacity {
            self.free_list_capacity = size
                .checked_add(size.max(1))
                .ok_or(RtreeError::LayoutOverflow)?;
        }
        self.free_list.push(node.clone());
        Ok(())
    }

    /// `MemoryUsage()`: `pages * sizeof( PAGE ) + capacity * sizeof( NODE* )` in
    /// the reference's accounting, not this port's heap use.
    pub fn memory_usage(&self) -> Result<u64, RtreeError> {
        let pages = u64::try_from(self.pages.len()).map_err(|_| RtreeError::LayoutOverflow)?;
        let page_size = Self::reference_page_size()?;
        pages
            .checked_mul(page_size)
            .and_then(|bytes| {
                self.free_list_capacity
                    .checked_mul(POINTER_SIZE)
                    .and_then(|list| bytes.checked_add(list))
            })
            .ok_or(RtreeError::LayoutOverflow)
    }

    /// `sizeof( PAGE )` in the pinned reference.
    pub fn reference_page_size() -> Result<u64, RtreeError> {
        Ok(Self::reference_page_layout()?.size)
    }

    /// `sizeof( PAGE )` and `alignof( PAGE )` in the pinned reference.
    pub fn reference_page_layout() -> Result<ReferenceLayout, RtreeError> {
        let per_page = u64::try_from(NODES_PER_PAGE).map_err(|_| RtreeError::LayoutOverflow)?;
        layout::page_layout(N::reference_layout()?, per_page)
    }

    /// `Owns( aNode )`: whether the node's page belongs to this allocator.  Freed
    /// slots of an owned page are still owned.
    pub fn owns(&self, node: &NodeHandle) -> bool {
        self.find_owning_page(node).is_some()
    }

    /// The private `findOwningPage`: the page index in this allocator and the slot
    /// offset, or `None` for a node of another (or a destroyed) allocator.
    pub fn find_owning_page(&self, node: &NodeHandle) -> Option<(usize, usize)> {
        let target = Weak::as_ptr(&node.page);
        self.pages
            .iter()
            .position(|page| core::ptr::eq(Rc::as_ptr(&page.token), target))
            .map(|index| (index, usize::from(node.slot)))
    }

    /// The node, if this allocator owns it and its slot is allocated.
    pub fn node(&self, node: &NodeHandle) -> Result<&N, RtreeError> {
        let (index, offset) = self.find_owning_page(node).ok_or(RtreeError::ForeignNode)?;
        let page = self.pages.get(index).ok_or(RtreeError::ForeignNode)?;
        if !page.is_used(offset) {
            return Err(RtreeError::NodeNotLive);
        }
        page.nodes.get(offset).ok_or(RtreeError::SlotOutOfRange)
    }

    /// Mutable access with the same conditions as [`Self::node`].
    pub fn node_mut(&mut self, node: &NodeHandle) -> Result<&mut N, RtreeError> {
        let (index, offset) = self.live(node)?;
        self.pages
            .get_mut(index)
            .and_then(|page| page.nodes.get_mut(offset))
            .ok_or(RtreeError::SlotOutOfRange)
    }

    fn live(&self, node: &NodeHandle) -> Result<(usize, usize), RtreeError> {
        let (index, offset) = self.find_owning_page(node).ok_or(RtreeError::ForeignNode)?;
        if !self
            .pages
            .get(index)
            .is_some_and(|page| page.is_used(offset))
        {
            return Err(RtreeError::NodeNotLive);
        }
        Ok((index, offset))
    }

    /// Two distinct live nodes at once, as the reference's callers use two
    /// pointers (for example a node and its copy or split sibling).
    pub fn node_pair_mut(
        &mut self,
        first: &NodeHandle,
        second: &NodeHandle,
    ) -> Result<(&mut N, &mut N), RtreeError> {
        let (page_a, offset_a) = self.live(first)?;
        let (page_b, offset_b) = self.live(second)?;
        if (page_a, offset_a) == (page_b, offset_b) {
            return Err(RtreeError::AliasedNodes);
        }
        let missing = RtreeError::SlotOutOfRange;
        if page_a == page_b {
            let nodes = &mut self.pages.get_mut(page_a).ok_or(missing)?.nodes;
            let (low, high) = nodes
                .split_at_mut_checked(offset_a.max(offset_b))
                .ok_or(missing)?;
            let low = low.get_mut(offset_a.min(offset_b)).ok_or(missing)?;
            let high = high.first_mut().ok_or(missing)?;
            return Ok(if offset_a < offset_b {
                (low, high)
            } else {
                (high, low)
            });
        }
        let (low, high) = self
            .pages
            .split_at_mut_checked(page_a.max(page_b))
            .ok_or(missing)?;
        let low = low
            .get_mut(page_a.min(page_b))
            .and_then(|page| {
                page.nodes
                    .get_mut(if page_a < page_b { offset_a } else { offset_b })
            })
            .ok_or(missing)?;
        let high = high
            .first_mut()
            .and_then(|page| {
                page.nodes
                    .get_mut(if page_a < page_b { offset_b } else { offset_a })
            })
            .ok_or(missing)?;
        Ok(if page_a < page_b {
            (low, high)
        } else {
            (high, low)
        })
    }

    /// Number of pages (`m_pages.size()`).
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// The page's `used` bitset as four 64-bit words, bit `i` of word `i / 64`
    /// for slot `i`.
    pub fn page_used_words(&self, page: usize) -> Option<[u64; WORDS]> {
        self.pages.get(page).map(|page| page.used)
    }

    /// The free list in order; the next allocation reuses the last entry.
    pub fn free_list(&self) -> &[NodeHandle] {
        &self.free_list
    }

    /// `m_freeList.capacity()` as libstdc++ would report it: starts at 0, a
    /// `push_back` into a full vector grows it to `size + max( size, 1 )`,
    /// `pop_back` keeps it, and a move transfers it and leaves 0 behind.
    pub fn free_list_capacity(&self) -> u64 {
        self.free_list_capacity
    }
}
