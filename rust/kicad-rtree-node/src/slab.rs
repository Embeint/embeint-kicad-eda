// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIRTREE::SLAB_ALLOCATOR` with safe, address-free node identity.
//!
//! In the reference, every allocator hands out `NODE*` pointers into its own 256-node
//! pages, and any allocator may `Free` and later re-`Allocate` any pointer to a
//! constructed node: the private `findOwningPage` simply fails for a node outside its
//! pages, so `markUnused` and `markUsed` do nothing while the node is still destroyed,
//! pushed, popped and placement-constructed where it lives.  That storage may be a slot
//! in another allocator's page (including one `PAGE()` constructed but nobody
//! allocated) or caller-owned storage such as a local `NODE n`.  All of it is one
//! address space.
//!
//! [`SlabArena`] is that shared space.  It owns every page, every piece of external
//! node storage and every allocator's state, and keeps apart what the C++ keeps apart:
//!
//! * **page ownership**: the list of pages each allocator owns (`m_pages`), which is
//!   what `Owns`, `findOwningPage` and `MemoryUsage` see; external storage is never
//!   owned;
//! * **slot occupancy**: each page's `used` bitset, changed only by its owner's
//!   `markUsed`/`markUnused` (the page scan reads it);
//! * **free-list entries**: each allocator's `m_freeList`, which may name any node,
//!   including one also named by another list;
//! * **object lifetime**: whether the node object currently exists.  `PAGE()` and
//!   [`SlabArena::new_external`] construct, `Free` destroys, `Allocate` re-constructs
//!   (also over an existing object, as placement new does).
//!
//! A [`NodeHandle`] is the arena's identity and a place: a page number and slot, or an
//! external storage number.  Numbers are never reused: destroyed storage keeps its
//! number without contents, so a handle into it is recognised exactly and never aliases
//! later storage.  No address is compared numerically or reaches any output.
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
///
/// The reference node is trivially destructible, which is what makes ending external
/// storage that holds a destroyed node, and constructing over an existing node,
/// defined.  Implementations must model such a type.
pub trait SlabNode {
    /// `new( node ) NODE()`.
    fn construct() -> Self;
    /// `sizeof( NODE )` and `alignof( NODE )` in the pinned Linux x86-64 reference.
    fn reference_layout() -> Result<ReferenceLayout, RtreeError>;
}

/// Identity of one arena; never compared by value.
#[derive(Debug)]
struct ArenaToken;

/// Where a node lives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Slot `slot` of page `page`.
    Page { page: u32, slot: u16 },
    /// Caller-owned storage outside every page, such as a local `NODE n`.
    External(u32),
}

/// An opaque `NODE*`: the arena and the node's place.
///
/// Handles compare equal exactly when they name the same storage, whichever allocator
/// owns its page now, and however the handle was obtained.
#[derive(Clone)]
pub struct NodeHandle {
    arena: Weak<ArenaToken>,
    place: Place,
}

impl PartialEq for NodeHandle {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.arena, &other.arena) && self.place == other.place
    }
}

impl Eq for NodeHandle {}

impl fmt::Debug for NodeHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Logical numbers only; never an address.
        match self.place {
            Place::Page { page, slot } => {
                write!(formatter, "NodeHandle(page {page}, slot {slot})")
            }
            Place::External(index) => write!(formatter, "NodeHandle(external {index})"),
        }
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

/// External node storage and whether its object currently exists.
struct External<N> {
    node: N,
    alive: bool,
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
}

/// `m_pages`, `m_freeList` and `m_freeList.capacity()` of one allocator.
#[derive(Default)]
struct AllocatorState {
    pages: Vec<u32>,
    free_list: Vec<NodeHandle>,
    /// libstdc++'s capacity, not Rust's: see [`SlabArena::free_list_capacity`].
    free_list_capacity: u64,
}

impl AllocatorState {
    /// Whether this allocator owns the node's page (`findOwningPage` succeeds).
    fn owns(&self, place: Place) -> bool {
        matches!(place, Place::Page { page, .. } if self.pages.contains(&page))
    }
}

/// The shared storage of every page, external node and allocator that may exchange
/// nodes.
pub struct SlabArena<N> {
    token: Rc<ArenaToken>,
    /// Indexed by page number; `None` once the page is destroyed.
    pages: Vec<Option<Page<N>>>,
    /// Indexed by external number; `None` once its storage has ended.
    externals: Vec<Option<External<N>>>,
    /// Indexed by allocator number; `None` once the allocator is destroyed.
    allocators: Vec<Option<AllocatorState>>,
}

impl<N> fmt::Debug for SlabArena<N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SlabArena")
            .field("pages", &self.pages.len())
            .field("externals", &self.externals.len())
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
            externals: Vec::new(),
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

    fn place(&self, node: &NodeHandle) -> Result<Place, RtreeError> {
        if !self.ours(&node.arena) {
            return Err(RtreeError::ForeignArena);
        }
        Ok(node.place)
    }

    fn page(&self, page: u32) -> Result<&Page<N>, RtreeError> {
        self.pages
            .get(page as usize)
            .and_then(Option::as_ref)
            .ok_or(RtreeError::NodeDestroyed)
    }

    fn page_mut(&mut self, page: u32) -> Result<&mut Page<N>, RtreeError> {
        self.pages
            .get_mut(page as usize)
            .and_then(Option::as_mut)
            .ok_or(RtreeError::NodeDestroyed)
    }

    fn external(&self, index: u32) -> Result<&External<N>, RtreeError> {
        self.externals
            .get(index as usize)
            .and_then(Option::as_ref)
            .ok_or(RtreeError::NodeDestroyed)
    }

    fn external_mut(&mut self, index: u32) -> Result<&mut External<N>, RtreeError> {
        self.externals
            .get_mut(index as usize)
            .and_then(Option::as_mut)
            .ok_or(RtreeError::NodeDestroyed)
    }

    /// Whether the object at the place exists; `NodeDestroyed` once its storage ended.
    fn alive_at(&self, place: Place) -> Result<bool, RtreeError> {
        match place {
            Place::Page { page, slot } => Ok(bit(&self.page(page)?.alive, usize::from(slot))),
            Place::External(index) => Ok(self.external(index)?.alive),
        }
    }

    /// End the object's lifetime (`~NODE()`, trivial).
    fn destroy_at(&mut self, place: Place) -> Result<(), RtreeError> {
        match place {
            Place::Page { page, slot } => {
                set_bit(&mut self.page_mut(page)?.alive, usize::from(slot), false);
            }
            Place::External(index) => self.external_mut(index)?.alive = false,
        }
        Ok(())
    }

    /// `new( node ) NODE()` at the place, over whatever object was there.
    fn construct_at(&mut self, place: Place) -> Result<(), RtreeError> {
        match place {
            Place::Page { page, slot } => {
                let page = self.page_mut(page)?;
                let offset = usize::from(slot);
                *page
                    .nodes
                    .get_mut(offset)
                    .ok_or(RtreeError::SlotOutOfRange)? = N::construct();
                set_bit(&mut page.alive, offset, true);
            }
            Place::External(index) => {
                let external = self.external_mut(index)?;
                external.node = N::construct();
                external.alive = true;
            }
        }
        Ok(())
    }

    /// `markUsed`/`markUnused`: only for a page the calling allocator owns.
    fn mark(&mut self, owned: bool, place: Place, value: bool) -> Result<(), RtreeError> {
        if let (true, Place::Page { page, slot }) = (owned, place) {
            set_bit(&mut self.page_mut(page)?.used, usize::from(slot), value);
        }
        Ok(())
    }

    fn handle(&self, place: Place) -> NodeHandle {
        NodeHandle {
            arena: Rc::downgrade(&self.token),
            place,
        }
    }

    fn page_handle(&self, page: u32, slot: usize) -> Result<NodeHandle, RtreeError> {
        let slot = u16::try_from(slot).map_err(|_| RtreeError::SlotOutOfRange)?;
        Ok(self.handle(Place::Page { page, slot }))
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

    /// A caller-owned node outside every page (`NODE n;`), default-constructed.  No
    /// allocator owns it, so freeing it or re-allocating it never touches a used bit.
    pub fn new_external(&mut self) -> Result<NodeHandle, RtreeError> {
        let index = u32::try_from(self.externals.len()).map_err(|_| RtreeError::LayoutOverflow)?;
        self.externals.push(Some(External {
            node: N::construct(),
            alive: true,
        }));
        Ok(self.handle(Place::External(index)))
    }

    /// End an external node's storage (its scope or deallocation), whether or not its
    /// object currently exists; defined because the node is trivially destructible.
    /// Handles to it, in free lists or node slots, become destroyed.
    pub fn destroy_external(&mut self, node: &NodeHandle) -> Result<(), RtreeError> {
        let Place::External(index) = self.place(node)? else {
            return Err(RtreeError::NotExternal);
        };
        self.external(index)?;
        if let Some(storage) = self.externals.get_mut(index as usize) {
            *storage = None;
        }
        Ok(())
    }

    /// `node + offset`: another element of the same `PAGE::nodes` array, such as a
    /// slot `PAGE()` constructed that no allocator has handed out.  The result must
    /// be inside the array; for external storage only offset 0 names an object.
    pub fn page_neighbour(&self, node: &NodeHandle, offset: i32) -> Result<NodeHandle, RtreeError> {
        match self.place(node)? {
            Place::Page { page, slot } => {
                self.page(page)?;
                let target = i64::from(slot) + i64::from(offset);
                let target = usize::try_from(target)
                    .ok()
                    .filter(|target| *target < NODES_PER_PAGE)
                    .ok_or(RtreeError::NeighbourOutOfPage)?;
                self.page_handle(page, target)
            }
            Place::External(index) => {
                self.external(index)?;
                if offset != 0 {
                    return Err(RtreeError::NeighbourOutOfPage);
                }
                Ok(node.clone())
            }
        }
    }

    /// `Allocate()`.
    ///
    /// The most recently freed node comes first, wherever it lives: `markUsed` sets its
    /// bit only if this allocator owns the page, and the node is constructed in place,
    /// even over an object that exists (another list may name the same slot).  A
    /// free-list entry whose storage has ended is refused (`NodeDestroyed`, state
    /// unchanged): the reference would construct into freed memory.  Otherwise the
    /// first unused slot of this allocator's first non-full page (which may be a slot
    /// someone else freed or re-constructed while its bit was clear), otherwise slot 0
    /// of a new page.
    pub fn allocate(&mut self, allocator: &AllocatorId) -> Result<NodeHandle, RtreeError> {
        let state = self.state(allocator)?;
        if let Some(top) = state.free_list.last().cloned() {
            let owned = state.owns(top.place);
            self.alive_at(top.place)?;
            self.state_mut(allocator)?.free_list.pop();
            self.mark(owned, top.place, true)?;
            self.construct_at(top.place)?;
            return Ok(top);
        }

        let mut found = None;
        for page in &state.pages {
            let storage = self.page(*page)?;
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
        let node = self.page_handle(page, offset)?;
        self.mark(true, node.place, true)?;
        self.construct_at(node.place)?;
        Ok(node)
    }

    /// `Free( aNode )`: a null node is ignored.  Otherwise `markUnused` clears the bit
    /// only if this allocator owns the node's page, the node is destroyed and it is
    /// appended to this allocator's free list, wherever it lives.
    ///
    /// Destroying a node whose object does not exist (a second destructor call) is
    /// refused with `NodeNotLive`, a node whose storage has ended with `NodeDestroyed`.
    pub fn free(
        &mut self,
        allocator: &AllocatorId,
        node: Option<&NodeHandle>,
    ) -> Result<(), RtreeError> {
        let state = self.state(allocator)?;
        let Some(node) = node else {
            return Ok(());
        };
        let owned = state.owns(self.place(node)?);
        if !self.alive_at(node.place)? {
            return Err(RtreeError::NodeNotLive);
        }
        // Clear the used bit so the page-scan fallback path stays consistent.
        self.mark(owned, node.place, false)?;
        self.destroy_at(node.place)?;

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

    /// `Owns( aNode )`: whether the node's page belongs to this allocator.  Null and
    /// external nodes are never owned; freed slots, and slots another allocator
    /// re-constructed, stay owned by the page's allocator.
    pub fn owns(
        &self,
        allocator: &AllocatorId,
        node: Option<&NodeHandle>,
    ) -> Result<bool, RtreeError> {
        Ok(self.find_owning_page(allocator, node)?.is_some())
    }

    /// The private `findOwningPage`: the page's index in this allocator and the slot,
    /// or `None` for null, external or another allocator's node.
    pub fn find_owning_page(
        &self,
        allocator: &AllocatorId,
        node: Option<&NodeHandle>,
    ) -> Result<Option<(usize, usize)>, RtreeError> {
        let state = self.state(allocator)?;
        let Some(node) = node else {
            return Ok(None);
        };
        if !self.ours(&node.arena) {
            return Ok(None);
        }
        let Place::Page { page, slot } = node.place else {
            return Ok(None);
        };
        Ok(state
            .pages
            .iter()
            .position(|owned| *owned == page)
            .map(|index| (index, usize::from(slot))))
    }

    /// Whether the handle's storage has ended (destroyed page or external storage).
    pub fn is_destroyed(&self, node: &NodeHandle) -> Result<bool, RtreeError> {
        match self.alive_at(self.place(node)?) {
            Ok(_) => Ok(false),
            Err(RtreeError::NodeDestroyed) => Ok(true),
            Err(error) => Err(error),
        }
    }

    /// Whether the node object currently exists (constructed and not destroyed).
    pub fn is_alive(&self, node: &NodeHandle) -> Result<bool, RtreeError> {
        self.alive_at(self.place(node)?)
    }

    /// The node, if its object exists.
    pub fn node(&self, node: &NodeHandle) -> Result<&N, RtreeError> {
        let place = self.place(node)?;
        if !self.alive_at(place)? {
            return Err(RtreeError::NodeNotLive);
        }
        match place {
            Place::Page { page, slot } => self
                .page(page)?
                .nodes
                .get(usize::from(slot))
                .ok_or(RtreeError::SlotOutOfRange),
            Place::External(index) => Ok(&self.external(index)?.node),
        }
    }

    /// Mutable access with the same conditions as [`Self::node`].
    pub fn node_mut(&mut self, node: &NodeHandle) -> Result<&mut N, RtreeError> {
        let place = self.place(node)?;
        if !self.alive_at(place)? {
            return Err(RtreeError::NodeNotLive);
        }
        match place {
            Place::Page { page, slot } => self
                .page_mut(page)?
                .nodes
                .get_mut(usize::from(slot))
                .ok_or(RtreeError::SlotOutOfRange),
            Place::External(index) => Ok(&mut self.external_mut(index)?.node),
        }
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
        match (first.place, second.place) {
            (Place::Page { page: a, slot: sa }, Place::Page { page: b, slot: sb }) => {
                let (page_a, offset_a) = (a as usize, usize::from(sa));
                let (page_b, offset_b) = (b as usize, usize::from(sb));
                if page_a == page_b {
                    let nodes = &mut self
                        .pages
                        .get_mut(page_a)
                        .and_then(Option::as_mut)
                        .ok_or(missing)?
                        .nodes;
                    return ordered_pair(nodes, offset_a, offset_b);
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
            (Place::External(a), Place::External(b)) => {
                let (a, b) = (a as usize, b as usize);
                let (low, high) = self
                    .externals
                    .split_at_mut_checked(a.max(b))
                    .ok_or(missing)?;
                let low = &mut low
                    .get_mut(a.min(b))
                    .and_then(Option::as_mut)
                    .ok_or(missing)?
                    .node;
                let high = &mut high
                    .first_mut()
                    .and_then(Option::as_mut)
                    .ok_or(missing)?
                    .node;
                Ok(if a < b { (low, high) } else { (high, low) })
            }
            (Place::Page { page, slot }, Place::External(index)) => {
                let page = self
                    .pages
                    .get_mut(page as usize)
                    .and_then(Option::as_mut)
                    .and_then(|page| page.nodes.get_mut(usize::from(slot)))
                    .ok_or(missing)?;
                let external = &mut self
                    .externals
                    .get_mut(index as usize)
                    .and_then(Option::as_mut)
                    .ok_or(missing)?
                    .node;
                Ok((page, external))
            }
            (Place::External(index), Place::Page { page, slot }) => {
                let external = &mut self
                    .externals
                    .get_mut(index as usize)
                    .and_then(Option::as_mut)
                    .ok_or(missing)?
                    .node;
                let page = self
                    .pages
                    .get_mut(page as usize)
                    .and_then(Option::as_mut)
                    .and_then(|page| page.nodes.get_mut(usize::from(slot)))
                    .ok_or(missing)?;
                Ok((external, page))
            }
        }
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
    /// name any node, including one in another list or in ended storage.
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

/// Two distinct elements of one slice, in argument order.
fn ordered_pair<T>(nodes: &mut [T], a: usize, b: usize) -> Result<(&mut T, &mut T), RtreeError> {
    let missing = RtreeError::SlotOutOfRange;
    let (low, high) = nodes.split_at_mut_checked(a.max(b)).ok_or(missing)?;
    let low = low.get_mut(a.min(b)).ok_or(missing)?;
    let high = high.first_mut().ok_or(missing)?;
    Ok(if a < b { (low, high) } else { (high, low) })
}
