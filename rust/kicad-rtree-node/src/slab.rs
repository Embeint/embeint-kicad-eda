// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIRTREE::SLAB_ALLOCATOR` with safe, address-free node identity.
//!
//! In the reference, every allocator hands out `NODE*` pointers into its own 256-node
//! pages, and any allocator may `Free` and later re-`Allocate` any node pointer: the
//! private `findOwningPage` simply fails for a foreign node, so `markUnused` and
//! `markUsed` do nothing while the node is still destroyed, pushed, popped and
//! re-constructed in the other allocator's page.  All allocators share one address
//! space.
//!
//! [`SlabArena`] is that shared space.  It owns every page and every allocator's state,
//! and keeps four things apart that the C++ keeps apart:
//!
//! * **page ownership**: the list of pages each allocator owns (`m_pages`), which is
//!   what `Owns`, `findOwningPage` and `MemoryUsage` see;
//! * **slot occupancy**: each page's `used` bitset, changed only by its owner's
//!   `markUsed`/`markUnused`;
//! * **free-list entries**: each allocator's `m_freeList`, which may name nodes in
//!   any allocator's pages;
//! * **object lifetime**: whether the node object in a slot currently exists.  `PAGE()`
//!   constructs all 256 nodes, `Free` destroys one, `Allocate` re-constructs one.
//!
//! A [`NodeHandle`] is the arena's identity, a page number and a slot.  Page numbers
//! are never reused: a destroyed page keeps its number without storage, so a handle
//! into it is recognised exactly and never aliases a later page.  No address is
//! compared numerically or reaches any output.
//!
//! An allocator is an [`AllocatorId`] inside the arena.  It lives until
//! [`SlabArena::destroy_allocator`] (the destructor) or until the arena is dropped.

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

/// Identity of one arena; never compared by value.
#[derive(Debug)]
struct ArenaToken;

/// An opaque `NODE*`: the arena, the page number and the slot offset in the page.
///
/// Handles compare equal exactly when they name the same slot of the same page,
/// whichever allocator owns that page now.
#[derive(Clone)]
pub struct NodeHandle {
    arena: Weak<ArenaToken>,
    page: u32,
    slot: u16,
}

impl PartialEq for NodeHandle {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.arena, &other.arena)
            && self.page == other.page
            && self.slot == other.slot
    }
}

impl Eq for NodeHandle {}

impl fmt::Debug for NodeHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Logical page number and slot only; never an address.
        write!(
            formatter,
            "NodeHandle(page {}, slot {})",
            self.page, self.slot
        )
    }
}

/// One `SLAB_ALLOCATOR` object inside an arena.
#[derive(Clone)]
pub struct AllocatorId {
    arena: Weak<ArenaToken>,
    index: u32,
}

impl PartialEq for AllocatorId {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.arena, &other.arena) && self.index == other.index
    }
}

impl Eq for AllocatorId {}

impl fmt::Debug for AllocatorId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "AllocatorId({})", self.index)
    }
}

/// `SLAB_ALLOCATOR::PAGE`: 256 nodes, the `used` bitset and their object lifetimes.
struct Page<N> {
    nodes: Vec<N>,
    used: [u64; WORDS],
    alive: [u64; WORDS],
}

fn bit(words: &[u64; WORDS], offset: usize) -> bool {
    words
        .get(offset / 64)
        .is_some_and(|word| word >> (offset % 64) & 1 == 1)
}

fn set_bit(words: &mut [u64; WORDS], offset: usize, value: bool) {
    if let Some(word) = words.get_mut(offset / 64) {
        let mask = 1_u64 << (offset % 64);
        if value {
            *word |= mask;
        } else {
            *word &= !mask;
        }
    }
}

impl<N: SlabNode> Page<N> {
    /// `PAGE() : used() {}`: every node is constructed, none is used.
    fn new() -> Self {
        Self {
            nodes: (0..NODES_PER_PAGE).map(|_| N::construct()).collect(),
            used: [0; WORDS],
            alive: [u64::MAX; WORDS],
        }
    }

    fn used_count(&self) -> usize {
        self.used
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    /// `new( node ) NODE()` into slot `offset`.
    fn construct(&mut self, offset: usize) -> Result<(), RtreeError> {
        *self
            .nodes
            .get_mut(offset)
            .ok_or(RtreeError::SlotOutOfRange)? = N::construct();
        set_bit(&mut self.alive, offset, true);
        Ok(())
    }
}

/// `m_pages`, `m_freeList` and `m_freeList.capacity()` of one allocator.
#[derive(Default)]
struct AllocatorState {
    pages: Vec<u32>,
    free_list: Vec<NodeHandle>,
    /// libstdc++'s capacity, not Rust's: see [`SlabArena::free_list_capacity`].
    free_list_capacity: u64,
}

/// The shared storage of every page and allocator that may exchange nodes.
pub struct SlabArena<N> {
    token: Rc<ArenaToken>,
    /// Indexed by page number; `None` once the page is destroyed.
    pages: Vec<Option<Page<N>>>,
    /// Indexed by allocator number; `None` once the allocator is destroyed.
    allocators: Vec<Option<AllocatorState>>,
}

impl<N> fmt::Debug for SlabArena<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SlabArena")
            .field("pages", &self.pages.len())
            .field("allocators", &self.allocators.len())
            .finish()
    }
}

impl<N: SlabNode> Default for SlabArena<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N: SlabNode> SlabArena<N> {
    pub fn new() -> Self {
        Self {
            token: Rc::new(ArenaToken),
            pages: Vec::new(),
            allocators: Vec::new(),
        }
    }

    fn ours(&self, arena: &Weak<ArenaToken>) -> bool {
        core::ptr::eq(Weak::as_ptr(arena), Rc::as_ptr(&self.token))
    }

    fn state(&self, allocator: &AllocatorId) -> Result<&AllocatorState, RtreeError> {
        if !self.ours(&allocator.arena) {
            return Err(RtreeError::ForeignArena);
        }
        self.allocators
            .get(allocator.index as usize)
            .and_then(Option::as_ref)
            .ok_or(RtreeError::AllocatorDestroyed)
    }

    fn state_mut(&mut self, allocator: &AllocatorId) -> Result<&mut AllocatorState, RtreeError> {
        if !self.ours(&allocator.arena) {
            return Err(RtreeError::ForeignArena);
        }
        self.allocators
            .get_mut(allocator.index as usize)
            .and_then(Option::as_mut)
            .ok_or(RtreeError::AllocatorDestroyed)
    }

    /// The page storage of a handle: `NodeDestroyed` once its page is destroyed.
    fn page(&self, node: &NodeHandle) -> Result<&Page<N>, RtreeError> {
        if !self.ours(&node.arena) {
            return Err(RtreeError::ForeignArena);
        }
        self.pages
            .get(node.page as usize)
            .ok_or(RtreeError::NodeDestroyed)?
            .as_ref()
            .ok_or(RtreeError::NodeDestroyed)
    }

    fn page_mut(&mut self, node: &NodeHandle) -> Result<&mut Page<N>, RtreeError> {
        if !self.ours(&node.arena) {
            return Err(RtreeError::ForeignArena);
        }
        self.pages
            .get_mut(node.page as usize)
            .ok_or(RtreeError::NodeDestroyed)?
            .as_mut()
            .ok_or(RtreeError::NodeDestroyed)
    }

    fn handle(&self, page: u32, slot: usize) -> Result<NodeHandle, RtreeError> {
        Ok(NodeHandle {
            arena: Rc::downgrade(&self.token),
            page,
            slot: u16::try_from(slot).map_err(|_| RtreeError::SlotOutOfRange)?,
        })
    }

    fn destroy_pages(&mut self, pages: &[u32]) {
        for page in pages {
            if let Some(storage) = self.pages.get_mut(*page as usize) {
                *storage = None;
            }
        }
    }

    /// `SLAB_ALLOCATOR() = default`.
    pub fn new_allocator(&mut self) -> Result<AllocatorId, RtreeError> {
        let index = u32::try_from(self.allocators.len()).map_err(|_| RtreeError::LayoutOverflow)?;
        self.allocators.push(Some(AllocatorState::default()));
        Ok(AllocatorId {
            arena: Rc::downgrade(&self.token),
            index,
        })
    }

    /// `~SLAB_ALLOCATOR()`: its pages and every node in them are destroyed.  Handles
    /// into them, in node slots or other allocators' free lists, become destroyed.
    pub fn destroy_allocator(&mut self, allocator: &AllocatorId) -> Result<(), RtreeError> {
        let state = core::mem::take(self.state_mut(allocator)?);
        self.destroy_pages(&state.pages);
        if let Some(slot) = self.allocators.get_mut(allocator.index as usize) {
            *slot = None;
        }
        Ok(())
    }

    /// `SLAB_ALLOCATOR( SLAB_ALLOCATOR&& )`: a new allocator takes the source's pages,
    /// free list and capacity; the source keeps none (libstdc++ moves leave the
    /// vectors empty with zero capacity).
    pub fn move_construct(&mut self, source: &AllocatorId) -> Result<AllocatorId, RtreeError> {
        let state = core::mem::take(self.state_mut(source)?);
        let target = self.new_allocator()?;
        *self.state_mut(&target)? = state;
        Ok(target)
    }

    /// `operator=( SLAB_ALLOCATOR&& )`: unless it is a self move (the reference's
    /// `this != &aOther` guard), the target's previous pages are destroyed and it
    /// takes the source's state, leaving the source empty.
    pub fn move_assign(
        &mut self,
        target: &AllocatorId,
        source: &AllocatorId,
    ) -> Result<(), RtreeError> {
        self.state(target)?;
        self.state(source)?;
        if target == source {
            return Ok(());
        }
        let moved = core::mem::take(self.state_mut(source)?);
        let previous = core::mem::replace(self.state_mut(target)?, moved);
        self.destroy_pages(&previous.pages);
        Ok(())
    }

    /// `Allocate()`.
    ///
    /// The most recently freed node comes first, whichever allocator's page holds it:
    /// `markUsed` sets its bit only if this allocator owns the page, and the node is
    /// re-constructed in place.  A free-list entry whose page was destroyed is refused
    /// (`NodeDestroyed`, state unchanged): the reference would construct into freed
    /// memory.  Otherwise the first unused slot of this allocator's first non-full
    /// page, otherwise slot 0 of a new page.
    pub fn allocate(&mut self, allocator: &AllocatorId) -> Result<NodeHandle, RtreeError> {
        let state = self.state(allocator)?;
        if let Some(top) = state.free_list.last().cloned() {
            let owned = state.pages.contains(&top.page);
            self.page(&top)?;
            let state = self.state_mut(allocator)?;
            state.free_list.pop();
            let page = self.page_mut(&top)?;
            let offset = usize::from(top.slot);
            if owned {
                set_bit(&mut page.used, offset, true);
            }
            page.construct(offset)?;
            return Ok(top);
        }

        let mut found = None;
        for page in &state.pages {
            let storage = self
                .pages
                .get(*page as usize)
                .and_then(Option::as_ref)
                .ok_or(RtreeError::NodeDestroyed)?;
            if storage.used_count() < NODES_PER_PAGE {
                found = (0..NODES_PER_PAGE)
                    .find(|offset| !bit(&storage.used, *offset))
                    .map(|offset| (*page, offset));
                if found.is_some() {
                    break;
                }
            }
        }
        let (page, offset) = match found {
            Some(found) => found,
            None => {
                // All pages full, allocate a new one
                let page =
                    u32::try_from(self.pages.len()).map_err(|_| RtreeError::LayoutOverflow)?;
                self.pages.push(Some(Page::new()));
                self.state_mut(allocator)?.pages.push(page);
                (page, 0)
            }
        };
        let node = self.handle(page, offset)?;
        let storage = self.page_mut(&node)?;
        set_bit(&mut storage.used, offset, true);
        storage.construct(offset)?;
        Ok(node)
    }

    /// `Free( aNode )`: a null node is ignored.  Otherwise `markUnused` clears the bit
    /// only if this allocator owns the node's page, the node is destroyed and it is
    /// appended to this allocator's free list, wherever its page lives.
    ///
    /// Destroying a node that does not exist (already freed: a second destructor call)
    /// is refused with `NodeNotLive`, a node in a destroyed page with `NodeDestroyed`.
    pub fn free(
        &mut self,
        allocator: &AllocatorId,
        node: Option<&NodeHandle>,
    ) -> Result<(), RtreeError> {
        let owned = {
            let state = self.state(allocator)?;
            let Some(node) = node else {
                return Ok(());
            };
            state.pages.contains(&node.page)
        };
        let Some(node) = node else {
            return Ok(());
        };
        let offset = usize::from(node.slot);
        let page = self.page_mut(node)?;
        if !bit(&page.alive, offset) {
            return Err(RtreeError::NodeNotLive);
        }
        // Clear the used bit so the page-scan fallback path stays consistent.
        if owned {
            set_bit(&mut page.used, offset, false);
        }
        set_bit(&mut page.alive, offset, false);

        // std::vector::push_back grows a full vector to size + max( size, 1 ).
        let state = self.state_mut(allocator)?;
        let size = u64::try_from(state.free_list.len()).map_err(|_| RtreeError::LayoutOverflow)?;
        if size == state.free_list_capacity {
            state.free_list_capacity = size
                .checked_add(size.max(1))
                .ok_or(RtreeError::LayoutOverflow)?;
        }
        state.free_list.push(node.clone());
        Ok(())
    }

    /// `MemoryUsage()`: `pages * sizeof( PAGE ) + capacity * sizeof( NODE* )` for this
    /// allocator's own pages, in the reference's accounting, not this port's heap use.
    pub fn memory_usage(&self, allocator: &AllocatorId) -> Result<u64, RtreeError> {
        let state = self.state(allocator)?;
        let pages = u64::try_from(state.pages.len()).map_err(|_| RtreeError::LayoutOverflow)?;
        let page_size = Self::reference_page_size()?;
        pages
            .checked_mul(page_size)
            .and_then(|bytes| {
                state
                    .free_list_capacity
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
    /// slots, and slots another allocator re-constructed, stay owned by the page's
    /// allocator.
    pub fn owns(&self, allocator: &AllocatorId, node: &NodeHandle) -> Result<bool, RtreeError> {
        Ok(self.find_owning_page(allocator, node)?.is_some())
    }

    /// The private `findOwningPage`: the page's index in this allocator and the slot,
    /// or `None` for a page it does not own.
    pub fn find_owning_page(
        &self,
        allocator: &AllocatorId,
        node: &NodeHandle,
    ) -> Result<Option<(usize, usize)>, RtreeError> {
        let state = self.state(allocator)?;
        if !self.ours(&node.arena) {
            return Ok(None);
        }
        Ok(state
            .pages
            .iter()
            .position(|page| *page == node.page)
            .map(|index| (index, usize::from(node.slot))))
    }

    /// Whether the handle's page has been destroyed.
    pub fn is_destroyed(&self, node: &NodeHandle) -> Result<bool, RtreeError> {
        match self.page(node) {
            Ok(_) => Ok(false),
            Err(RtreeError::NodeDestroyed) => Ok(true),
            Err(error) => Err(error),
        }
    }

    /// Whether the node object currently exists (constructed and not destroyed).
    pub fn is_alive(&self, node: &NodeHandle) -> Result<bool, RtreeError> {
        Ok(bit(&self.page(node)?.alive, usize::from(node.slot)))
    }

    /// The node, if its object exists.
    pub fn node(&self, node: &NodeHandle) -> Result<&N, RtreeError> {
        let offset = usize::from(node.slot);
        let page = self.page(node)?;
        if !bit(&page.alive, offset) {
            return Err(RtreeError::NodeNotLive);
        }
        page.nodes.get(offset).ok_or(RtreeError::SlotOutOfRange)
    }

    /// Mutable access with the same conditions as [`Self::node`].
    pub fn node_mut(&mut self, node: &NodeHandle) -> Result<&mut N, RtreeError> {
        let offset = usize::from(node.slot);
        let page = self.page_mut(node)?;
        if !bit(&page.alive, offset) {
            return Err(RtreeError::NodeNotLive);
        }
        page.nodes.get_mut(offset).ok_or(RtreeError::SlotOutOfRange)
    }

    /// Two distinct existing nodes at once, as the reference's callers use two
    /// pointers (for example a node and its copy or split sibling).
    pub fn node_pair_mut(
        &mut self,
        first: &NodeHandle,
        second: &NodeHandle,
    ) -> Result<(&mut N, &mut N), RtreeError> {
        self.node(first)?;
        self.node(second)?;
        if first == second {
            return Err(RtreeError::AliasedNodes);
        }
        let missing = RtreeError::SlotOutOfRange;
        let (page_a, offset_a) = (first.page as usize, usize::from(first.slot));
        let (page_b, offset_b) = (second.page as usize, usize::from(second.slot));
        if page_a == page_b {
            let nodes = &mut self
                .pages
                .get_mut(page_a)
                .and_then(Option::as_mut)
                .ok_or(missing)?
                .nodes;
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
        let (low_offset, high_offset) = if page_a < page_b {
            (offset_a, offset_b)
        } else {
            (offset_b, offset_a)
        };
        let low = low
            .get_mut(page_a.min(page_b))
            .and_then(Option::as_mut)
            .and_then(|page| page.nodes.get_mut(low_offset))
            .ok_or(missing)?;
        let high = high
            .first_mut()
            .and_then(Option::as_mut)
            .and_then(|page| page.nodes.get_mut(high_offset))
            .ok_or(missing)?;
        Ok(if page_a < page_b {
            (low, high)
        } else {
            (high, low)
        })
    }

    /// Number of pages the allocator owns (`m_pages.size()`).
    pub fn page_count(&self, allocator: &AllocatorId) -> Result<usize, RtreeError> {
        Ok(self.state(allocator)?.pages.len())
    }

    /// The allocator's `index`-th page's `used` bitset as four 64-bit words, bit `i`
    /// of word `i / 64` for slot `i`.
    pub fn page_used_words(
        &self,
        allocator: &AllocatorId,
        index: usize,
    ) -> Result<Option<[u64; WORDS]>, RtreeError> {
        let state = self.state(allocator)?;
        Ok(state
            .pages
            .get(index)
            .and_then(|page| self.pages.get(*page as usize))
            .and_then(Option::as_ref)
            .map(|page| page.used))
    }

    /// The free list in order; the next allocation reuses the last entry.  Entries may
    /// name nodes in other allocators' pages, including destroyed ones.
    pub fn free_list(&self, allocator: &AllocatorId) -> Result<&[NodeHandle], RtreeError> {
        Ok(&self.state(allocator)?.free_list)
    }

    /// `m_freeList.capacity()` as libstdc++ would report it: starts at 0, a
    /// `push_back` into a full vector grows it to `size + max( size, 1 )`,
    /// `pop_back` keeps it, and a move transfers it and leaves 0 behind.
    pub fn free_list_capacity(&self, allocator: &AllocatorId) -> Result<u64, RtreeError> {
        Ok(self.state(allocator)?.free_list_capacity)
    }
}
