// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/observable.cpp`: `UTIL::DETAIL::OBSERVABLE_BASE`, its `IMPL`
//! subscription list and the RAII subscription handle `UTIL::LINK`.
//!
//! The reference shares one `IMPL` between observables (copy construction) and links
//! through `std::shared_ptr`, and identifies observers by `void*`. Here:
//!
//! * every `IMPL` lives in an [`ObservableArena`] slot with an explicit use count;
//! * one [`ImplRef`] is one `shared_ptr<IMPL>` copy. It is neither `Clone` nor
//!   `Copy`; copies are made with the arena and released with it, which is where the
//!   reference's constructors and destructors adjust `use_count()`;
//! * a handle is branded with its arena's caller-supplied [`ArenaId`] and with the
//!   generation of its slot, so a handle from another arena, or one that outlived its
//!   `IMPL`, is refused with a typed error before anything is changed. The reference
//!   cannot express either mistake (a `shared_ptr` carries its own control block);
//! * an observer pointer is an [`ObserverPtr`], `None` being `nullptr`.
//!
//! The release (`NDEBUG`) behaviour of the reference is ported: `assert` is a no-op
//! there, so removing an observer that is not subscribed does nothing and adding an
//! observer while the iteration count is nonzero still appends. Dereferencing a null
//! `impl_` in `OBSERVABLE_BASE::remove_observer` is undefined in the reference and
//! returns [`ObservableError::NoImpl`] here.
//!
//! The `OBSERVABLE<T>` template (`Subscribe`, `Notify`, ...) is a header body owned by
//! [`crate::HEADER_UNIT`] and ported in [`crate::observable_template`].
//! [`ObservableArena::observers`], [`ObservableArena::structure_revision`] and
//! [`ImplRef::id`] expose the `impl_->observers_` access that template needs.

/// Identity of an observer object: the reference's `void*` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObserverId(pub u32);

/// A subscription list entry: `None` is `nullptr` (an empty or removed slot).
pub type ObserverPtr = Option<ObserverId>;

/// Failures where the reference has no defined behaviour, or a handle is unusable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservableError {
    /// `OBSERVABLE_BASE::remove_observer` with a null `impl_` (the reference
    /// asserts, then dereferences null).
    NoImpl,
    /// The handle belongs to this arena but its `IMPL` no longer exists.
    StaleHandle,
    /// The handle was issued by a different arena.
    ForeignHandle,
    /// The subscription list could not grow (the reference throws `bad_alloc`).
    Allocation,
}

/// Identity of an [`ObservableArena`], chosen by whoever creates it. Arenas whose
/// handles may meet must be given distinct identities; nothing global allocates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArenaId(pub u32);

/// Stable identity of a live `IMPL` (the reference's `IMPL*`), for comparisons only.
/// It names the arena, the slot and the slot's generation, so an `IMPL` created
/// later in a reused slot never compares equal to a destroyed one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImplId {
    arena: ArenaId,
    index: usize,
    generation: u32,
}

/// One `std::shared_ptr<OBSERVABLE_BASE::IMPL>` copy owned by an observable or link.
#[derive(Debug, PartialEq, Eq)]
#[must_use = "an ImplRef is a counted reference; release it through the arena"]
pub struct ImplRef(ImplId);

impl ImplRef {
    /// The identity of the referenced `IMPL`.
    pub fn id(&self) -> ImplId {
        self.0
    }
}

/// `OBSERVABLE_BASE::IMPL`.
#[derive(Debug)]
struct Impl {
    observers: Vec<ObserverPtr>,
    iteration_count: u32,
    /// `owned_by_ != nullptr`. The owner pointer itself is never dereferenced.
    owned: bool,
    /// `shared_ptr::use_count()`.
    use_count: usize,
    /// Count of changes to the number of entries of `observers` (a `push_back`, or an
    /// `erase` that removed at least one entry). Not reference state: it lets the
    /// `OBSERVABLE<T>` loop detect the iterator invalidation the reference leaves
    /// undefined.
    revision: u64,
}

impl Impl {
    /// `IMPL::is_shared`.
    fn is_shared(&self) -> bool {
        !self.owned
    }

    /// `IMPL::set_shared`.
    fn set_shared(&mut self) {
        self.owned = false;
    }

    /// `IMPL::enter_iteration` (`unsigned int` arithmetic is modular).
    fn enter_iteration(&mut self) {
        self.iteration_count = self.iteration_count.wrapping_add(1);
    }

    /// `IMPL::leave_iteration`.
    fn leave_iteration(&mut self) {
        self.iteration_count = self.iteration_count.wrapping_sub(1);

        if self.iteration_count == 0 {
            self.collect();
        }
    }

    /// `IMPL::is_iterating`.
    fn is_iterating(&self) -> bool {
        self.iteration_count != 0
    }

    /// `IMPL::add_observer`. `assert( !is_iterating() )` is a release no-op.
    fn add_observer(&mut self, observer: ObserverPtr) -> Result<(), ObservableError> {
        self.observers
            .try_reserve(1)
            .map_err(|_| ObservableError::Allocation)?;
        self.observers.push(observer);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// `IMPL::remove_observer`: the first matching entry is nulled while iterating,
    /// otherwise erased. A missing observer is `assert( false )`, a release no-op.
    fn remove_observer(&mut self, observer: ObserverPtr) {
        let Some(index) = self.observers.iter().position(|o| *o == observer) else {
            return;
        };

        if self.is_iterating() {
            if let Some(slot) = self.observers.get_mut(index) {
                *slot = None;
            }
        } else {
            self.observers.remove(index);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// `IMPL::collect`: drop null entries, preserving order (`remove_if` + `erase`).
    fn collect(&mut self) {
        let before = self.observers.len();
        self.observers.retain(Option::is_some);

        if self.observers.len() != before {
            self.revision = self.revision.wrapping_add(1);
        }
    }
}

/// One arena slot: the `IMPL` it holds, if any, and how many `IMPL`s it has held.
#[derive(Debug)]
struct Slot {
    generation: u32,
    value: Option<Impl>,
}

/// Storage for every `IMPL`, replacing the reference's heap and reference counts.
#[derive(Debug)]
pub struct ObservableArena {
    id: ArenaId,
    slots: Vec<Slot>,
    /// Freed slots, reused last-in first-out so identities are deterministic.
    free: Vec<usize>,
}

/// `UTIL::DETAIL::OBSERVABLE_BASE`: holds `impl_`, possibly null.
#[derive(Debug, Default)]
#[must_use = "an ObservableBase may hold an ImplRef; release it with drop_observable"]
pub struct ObservableBase {
    impl_: Option<ImplRef>,
}

/// `UTIL::LINK`: a subscription handle holding `token_` and `observer_`.
#[derive(Debug, Default)]
#[must_use = "a Link may hold an ImplRef; release it with drop_link"]
pub struct Link {
    token: Option<ImplRef>,
    observer: ObserverPtr,
}

impl ObservableBase {
    /// `OBSERVABLE_BASE()`: no `impl_`.
    pub fn new() -> Self {
        Self { impl_: None }
    }

    /// The current `impl_`, for introspection.
    pub fn impl_ref(&self) -> Option<&ImplRef> {
        self.impl_.as_ref()
    }
}

impl Link {
    /// `LINK()`: no token and a null observer.
    pub fn new() -> Self {
        Self {
            token: None,
            observer: None,
        }
    }

    /// `LINK( std::shared_ptr<IMPL> token, void* observer )`. The token may be null.
    pub fn with_token(token: Option<ImplRef>, observer: ObserverPtr) -> Self {
        Self { token, observer }
    }

    /// `LINK( LINK&& other )`: take the token and copy the observer. The moved-from
    /// link keeps its `observer_` value and has a null token.
    pub fn move_from(other: &mut Link) -> Self {
        Self {
            token: other.token.take(),
            observer: other.observer,
        }
    }

    /// `LINK::operator bool`.
    pub fn is_linked(&self) -> bool {
        self.token.is_some()
    }

    /// The private `token_`, for introspection.
    pub fn token(&self) -> Option<&ImplRef> {
        self.token.as_ref()
    }

    /// The private `observer_`, for introspection.
    pub fn observer(&self) -> ObserverPtr {
        self.observer
    }
}

impl ObservableArena {
    /// An empty arena identified by `id`, which brands every handle it issues.
    pub fn new(id: ArenaId) -> Self {
        Self {
            id,
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    /// The identity this arena brands its handles with.
    pub fn id(&self) -> ArenaId {
        self.id
    }

    /// The slot `id` names, refusing a foreign or stale handle.
    fn slot(&self, id: ImplId) -> Result<&Slot, ObservableError> {
        if id.arena != self.id {
            return Err(ObservableError::ForeignHandle);
        }
        match self.slots.get(id.index) {
            Some(slot) if slot.generation == id.generation && slot.value.is_some() => Ok(slot),
            _ => Err(ObservableError::StaleHandle),
        }
    }

    fn get(&self, id: ImplId) -> Result<&Impl, ObservableError> {
        self.slot(id)?
            .value
            .as_ref()
            .ok_or(ObservableError::StaleHandle)
    }

    fn get_mut(&mut self, id: ImplId) -> Result<&mut Impl, ObservableError> {
        self.slot(id)?;
        self.slots
            .get_mut(id.index)
            .and_then(|slot| slot.value.as_mut())
            .ok_or(ObservableError::StaleHandle)
    }

    /// True when `id` names a live `IMPL` of this arena.
    pub fn is_live(&self, id: ImplId) -> bool {
        self.slot(id).is_ok()
    }

    /// `std::make_shared<IMPL>( owned_by )`.
    fn make_impl(&mut self, owned: bool) -> Result<ImplRef, ObservableError> {
        let value = Impl {
            observers: Vec::new(),
            iteration_count: 0,
            owned,
            use_count: 1,
            revision: 0,
        };

        let arena = self.id;

        if let Some(index) = self.free.pop() {
            if let Some(slot) = self.slots.get_mut(index) {
                slot.value = Some(value);
                return Ok(ImplRef(ImplId {
                    arena,
                    index,
                    generation: slot.generation,
                }));
            }
        }

        self.slots
            .try_reserve(1)
            .map_err(|_| ObservableError::Allocation)?;
        self.slots.push(Slot {
            generation: 0,
            value: Some(value),
        });
        Ok(ImplRef(ImplId {
            arena,
            index: self.slots.len() - 1,
            generation: 0,
        }))
    }

    /// Copy a `shared_ptr<IMPL>`.
    fn copy_ref(&mut self, reference: &ImplRef) -> Result<ImplRef, ObservableError> {
        let value = self.get_mut(reference.id())?;
        // Every count is backed by a distinct live handle, so it cannot overflow.
        value.use_count = value.use_count.saturating_add(1);
        Ok(ImplRef(reference.0))
    }

    /// Destroy a `shared_ptr<IMPL>` copy, destroying the `IMPL` with the last one.
    /// A foreign or stale handle is refused before anything changes.
    fn release(&mut self, reference: ImplRef) -> Result<(), ObservableError> {
        let id = reference.id();
        let value = self.get_mut(id)?;
        value.use_count = value.use_count.saturating_sub(1);

        if value.use_count == 0 {
            if let Some(slot) = self.slots.get_mut(id.index) {
                slot.value = None;
                // A slot whose generation would wrap is retired instead of reused, so
                // an old handle can never name a newer `IMPL`.
                if let Some(next) = slot.generation.checked_add(1) {
                    slot.generation = next;
                    self.free.push(id.index);
                }
            }
        }

        Ok(())
    }

    // ---- OBSERVABLE_BASE ------------------------------------------------------------

    /// `OBSERVABLE_BASE( OBSERVABLE_BASE& other )`: share `other`'s list, marking it
    /// shared (allocating a shared one if `other` has none).
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`], [`ObservableError::StaleHandle`] or
    /// [`ObservableError::Allocation`].
    pub fn copy_observable(
        &mut self,
        other: &mut ObservableBase,
    ) -> Result<ObservableBase, ObservableError> {
        Ok(ObservableBase {
            impl_: Some(self.get_shared_impl(other)?),
        })
    }

    /// `~OBSERVABLE_BASE()`: release `impl_`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn drop_observable(&mut self, base: ObservableBase) -> Result<(), ObservableError> {
        match base.impl_ {
            Some(reference) => self.release(reference),
            None => Ok(()),
        }
    }

    /// `OBSERVABLE_BASE::allocate_impl`: create an owned (non-shared) list if none.
    ///
    /// # Errors
    /// [`ObservableError::Allocation`].
    pub fn allocate_impl(&mut self, base: &mut ObservableBase) -> Result<(), ObservableError> {
        if base.impl_.is_none() {
            base.impl_ = Some(self.make_impl(true)?);
        }

        Ok(())
    }

    /// `OBSERVABLE_BASE::allocate_shared_impl`: create a shared list if none, else
    /// mark the existing list shared.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`], [`ObservableError::StaleHandle`] or
    /// [`ObservableError::Allocation`].
    pub fn allocate_shared_impl(
        &mut self,
        base: &mut ObservableBase,
    ) -> Result<(), ObservableError> {
        match &base.impl_ {
            None => base.impl_ = Some(self.make_impl(false)?),
            Some(reference) => self.get_mut(reference.id())?.set_shared(),
        }

        Ok(())
    }

    /// `OBSERVABLE_BASE::deallocate_impl`: `impl_.reset()`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`]; `impl_`
    /// is then left unchanged.
    pub fn deallocate_impl(&mut self, base: &mut ObservableBase) -> Result<(), ObservableError> {
        if let Some(reference) = &base.impl_ {
            self.get(reference.id())?;
        }

        match base.impl_.take() {
            Some(reference) => self.release(reference),
            None => Ok(()),
        }
    }

    /// `OBSERVABLE_BASE::get_shared_impl`: `allocate_shared_impl()`, then a copy of
    /// `impl_`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`], [`ObservableError::StaleHandle`] or
    /// [`ObservableError::Allocation`].
    pub fn get_shared_impl(
        &mut self,
        base: &mut ObservableBase,
    ) -> Result<ImplRef, ObservableError> {
        self.allocate_shared_impl(base)?;
        match &base.impl_ {
            Some(reference) => self.copy_ref(reference),
            None => Err(ObservableError::StaleHandle),
        }
    }

    /// A copy of `impl_` (the protected member a derived `OBSERVABLE<T>` reads, as in
    /// `Subscribe`'s `LINK( impl_, ... )`). `None` when `impl_` is null.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn share_impl(
        &mut self,
        base: &ObservableBase,
    ) -> Result<Option<ImplRef>, ObservableError> {
        base.impl_
            .as_ref()
            .map(|reference| self.copy_ref(reference))
            .transpose()
    }

    /// `OBSERVABLE_BASE::add_observer`: `allocate_impl()`, then append.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`], [`ObservableError::StaleHandle`] or
    /// [`ObservableError::Allocation`].
    pub fn add_observer(
        &mut self,
        base: &mut ObservableBase,
        observer: ObserverPtr,
    ) -> Result<(), ObservableError> {
        self.allocate_impl(base)?;
        let id = base
            .impl_
            .as_ref()
            .map(ImplRef::id)
            .ok_or(ObservableError::StaleHandle)?;
        self.get_mut(id)?.add_observer(observer)
    }

    /// `OBSERVABLE_BASE::remove_observer`.
    ///
    /// # Errors
    /// [`ObservableError::NoImpl`] when `impl_` is null (undefined in the reference);
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn remove_observer(
        &mut self,
        base: &ObservableBase,
        observer: ObserverPtr,
    ) -> Result<(), ObservableError> {
        let id = base
            .impl_
            .as_ref()
            .map(ImplRef::id)
            .ok_or(ObservableError::NoImpl)?;
        self.get_mut(id)?.remove_observer(observer);
        Ok(())
    }

    /// `OBSERVABLE_BASE::enter_iteration`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn enter_iteration(&mut self, base: &ObservableBase) -> Result<(), ObservableError> {
        if let Some(reference) = &base.impl_ {
            self.get_mut(reference.id())?.enter_iteration();
        }

        Ok(())
    }

    /// `OBSERVABLE_BASE::leave_iteration`: after the list's own `leave_iteration`, a
    /// list that is not iterating, not shared and referenced only by this observable
    /// is released, even if it still holds observers.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn leave_iteration(&mut self, base: &mut ObservableBase) -> Result<(), ObservableError> {
        let Some(id) = base.impl_.as_ref().map(ImplRef::id) else {
            return Ok(());
        };
        let value = self.get_mut(id)?;
        value.leave_iteration();

        if !value.is_iterating() && !value.is_shared() && value.use_count == 1 {
            self.deallocate_impl(base)?;
        }

        Ok(())
    }

    /// `OBSERVABLE_BASE::size`: entries in the list, including nulled ones.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn size(&self, base: &ObservableBase) -> Result<usize, ObservableError> {
        match &base.impl_ {
            Some(reference) => Ok(self.get(reference.id())?.observers.len()),
            None => Ok(0),
        }
    }

    /// `OBSERVABLE_BASE::on_observers_empty`: `deallocate_impl()`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn on_observers_empty(&mut self, base: &mut ObservableBase) -> Result<(), ObservableError> {
        self.deallocate_impl(base)
    }

    // ---- IMPL introspection (impl_->observers_ etc.) -----------------------------------

    /// `impl_->observers_` of a live list.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn observers(&self, id: ImplId) -> Result<&[ObserverPtr], ObservableError> {
        Ok(&self.get(id)?.observers)
    }

    /// `IMPL::iteration_count_`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn iteration_count(&self, id: ImplId) -> Result<u32, ObservableError> {
        Ok(self.get(id)?.iteration_count)
    }

    /// `IMPL::is_shared`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn is_shared(&self, id: ImplId) -> Result<bool, ObservableError> {
        Ok(self.get(id)?.is_shared())
    }

    /// `IMPL::is_iterating`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn is_iterating(&self, id: ImplId) -> Result<bool, ObservableError> {
        Ok(self.get(id)?.is_iterating())
    }

    /// The structural revision of a live list: it changes whenever an entry is
    /// appended or erased (nulling an entry leaves it unchanged). Not reference state.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn structure_revision(&self, id: ImplId) -> Result<u64, ObservableError> {
        Ok(self.get(id)?.revision)
    }

    /// `shared_ptr<IMPL>::use_count()`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn use_count(&self, id: ImplId) -> Result<usize, ObservableError> {
        Ok(self.get(id)?.use_count)
    }

    // ---- LINK ---------------------------------------------------------------------------

    /// `LINK::operator=( LINK&& other )` for distinct links: `reset()`, take the token,
    /// copy the observer. The moved-from link keeps its `observer_` value.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`] for
    /// either link's token, before either link changes.
    pub fn move_assign_link(
        &mut self,
        this: &mut Link,
        other: &mut Link,
    ) -> Result<(), ObservableError> {
        if let Some(token) = &other.token {
            self.get(token.id())?;
        }
        self.reset_link(this)?;
        this.token = other.token.take();
        this.observer = other.observer;
        Ok(())
    }

    /// `LINK::operator=( LINK&& other )` between elements of `links`, including the
    /// self-assignment branch (`this == &other`), which changes nothing. Indices out
    /// of range are a stale handle.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn move_assign_link_in(
        &mut self,
        links: &mut [Link],
        this: usize,
        other: usize,
    ) -> Result<(), ObservableError> {
        if this >= links.len() || other >= links.len() {
            return Err(ObservableError::StaleHandle);
        }

        if this == other {
            return Ok(());
        }

        let (low, high) = links.split_at_mut(this.max(other));
        let (lower, upper) = match (low.get_mut(this.min(other)), high.first_mut()) {
            (Some(lower), Some(upper)) => (lower, upper),
            _ => return Err(ObservableError::StaleHandle),
        };

        if this < other {
            self.move_assign_link(lower, upper)
        } else {
            self.move_assign_link(upper, lower)
        }
    }

    /// `LINK::reset`: remove this observer from the token's list (first match,
    /// nulled while iterating) and release the token.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`]; the link
    /// and every list are then left unchanged.
    pub fn reset_link(&mut self, link: &mut Link) -> Result<(), ObservableError> {
        let Some(id) = link.token.as_ref().map(ImplRef::id) else {
            return Ok(());
        };
        self.get_mut(id)?.remove_observer(link.observer);

        match link.token.take() {
            Some(token) => self.release(token),
            None => Ok(()),
        }
    }

    /// `~LINK()`: `reset()`.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`] or [`ObservableError::StaleHandle`].
    pub fn drop_link(&mut self, mut link: Link) -> Result<(), ObservableError> {
        self.reset_link(&mut link)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: ObserverPtr = Some(ObserverId(1));

    /// qa/tests/common/test_observable.cpp: MoveAssignedLinkLeavesOldObservable.
    #[test]
    fn move_assigned_link_leaves_old_observable() -> Result<(), ObservableError> {
        let mut arena = ObservableArena::new(ArenaId(0));
        let mut first = ObservableBase::new();
        let mut second = ObservableBase::new();

        arena.add_observer(&mut first, A)?;
        let mut link = Link::with_token(arena.share_impl(&first)?, A);
        arena.add_observer(&mut second, A)?;
        let mut other = Link::with_token(arena.share_impl(&second)?, A);
        arena.move_assign_link(&mut link, &mut other)?;
        arena.drop_link(other)?;

        assert_eq!(arena.size(&first)?, 0);
        assert_eq!(arena.size(&second)?, 1);

        arena.drop_link(link)?;
        arena.drop_observable(first)?;
        arena.drop_observable(second)?;
        assert!(arena.slots.iter().all(|slot| slot.value.is_none()));
        Ok(())
    }

    /// After a notification, an unshared list with no link is released even though it
    /// still has an unmanaged subscriber.
    #[test]
    fn leave_iteration_releases_unshared_unlinked_list() -> Result<(), ObservableError> {
        let mut arena = ObservableArena::new(ArenaId(0));
        let mut base = ObservableBase::new();
        arena.add_observer(&mut base, A)?;
        arena.enter_iteration(&base)?;
        arena.leave_iteration(&mut base)?;
        assert!(base.impl_ref().is_none());
        assert_eq!(
            arena.remove_observer(&base, A),
            Err(ObservableError::NoImpl)
        );
        Ok(())
    }

    #[test]
    fn removal_while_iterating_nulls_then_collects() -> Result<(), ObservableError> {
        let mut arena = ObservableArena::new(ArenaId(0));
        let mut base = ObservableBase::new();
        arena.add_observer(&mut base, A)?;
        arena.add_observer(&mut base, Some(ObserverId(2)))?;
        let mut copy = arena.copy_observable(&mut base)?;
        arena.enter_iteration(&base)?;
        arena.remove_observer(&base, A)?;
        assert_eq!(arena.size(&base)?, 2);
        arena.leave_iteration(&mut base)?;
        assert_eq!(arena.size(&copy)?, 1);
        let id = copy
            .impl_ref()
            .map(ImplRef::id)
            .ok_or(ObservableError::StaleHandle)?;
        assert!(arena.is_shared(id)?);
        assert_eq!(arena.use_count(id)?, 2);
        arena.on_observers_empty(&mut copy)?;
        assert_eq!(arena.use_count(id)?, 1);
        arena.drop_observable(copy)?;
        arena.drop_observable(base)?;
        Ok(())
    }

    /// Two arenas each hold a list in slot 0. A link of arena `a` handed to arena `b`
    /// is refused before anything changes: `b`'s live list is not freed and `a`'s
    /// count is untouched (review reproduction).
    #[test]
    fn foreign_handles_are_refused_before_any_change() -> Result<(), ObservableError> {
        let mut a = ObservableArena::new(ArenaId(1));
        let mut b = ObservableArena::new(ArenaId(2));
        let mut base_a = ObservableBase::new();
        let mut base_b = ObservableBase::new();
        a.add_observer(&mut base_a, A)?;
        b.add_observer(&mut base_b, A)?;
        let mut link_a = Link::with_token(a.share_impl(&base_a)?, A);
        let id_a = link_a
            .token()
            .map(ImplRef::id)
            .ok_or(ObservableError::StaleHandle)?;
        let id_b = base_b
            .impl_ref()
            .map(ImplRef::id)
            .ok_or(ObservableError::StaleHandle)?;

        assert_eq!(
            b.reset_link(&mut link_a),
            Err(ObservableError::ForeignHandle)
        );
        assert!(link_a.is_linked());
        assert_eq!(a.use_count(id_a)?, 2);
        assert_eq!(a.observers(id_a)?, &[A]);
        assert_eq!(b.use_count(id_b)?, 1);
        assert_eq!(b.observers(id_b)?, &[A]);
        assert_eq!(b.use_count(id_a), Err(ObservableError::ForeignHandle));
        assert_eq!(
            b.deallocate_impl(&mut base_a),
            Err(ObservableError::ForeignHandle)
        );
        assert!(base_a.impl_ref().is_some());
        assert_eq!(
            b.remove_observer(&base_a, A),
            Err(ObservableError::ForeignHandle)
        );
        assert_eq!(
            b.add_observer(&mut base_a, A),
            Err(ObservableError::ForeignHandle)
        );
        let mut empty = Link::new();
        assert_eq!(
            b.move_assign_link(&mut empty, &mut link_a),
            Err(ObservableError::ForeignHandle)
        );
        assert!(link_a.is_linked());
        assert_eq!(a.observers(id_a)?, &[A]);

        a.drop_link(link_a)?;
        assert_eq!(a.use_count(id_a)?, 1);
        a.drop_observable(base_a)?;
        b.drop_observable(base_b)?;
        Ok(())
    }

    /// A handle that outlived its list is stale even after the slot is reused.
    #[test]
    fn stale_handles_are_refused_after_slot_reuse() -> Result<(), ObservableError> {
        let mut arena = ObservableArena::new(ArenaId(0));
        let mut first = ObservableBase::new();
        arena.add_observer(&mut first, A)?;
        let old = first
            .impl_ref()
            .map(ImplRef::id)
            .ok_or(ObservableError::StaleHandle)?;
        arena.on_observers_empty(&mut first)?;
        assert!(!arena.is_live(old));
        let mut second = ObservableBase::new();
        arena.add_observer(&mut second, A)?;
        let new = second
            .impl_ref()
            .map(ImplRef::id)
            .ok_or(ObservableError::StaleHandle)?;
        assert_ne!(old, new);
        assert_eq!(arena.use_count(old), Err(ObservableError::StaleHandle));
        assert_eq!(arena.use_count(new)?, 1);
        arena.drop_observable(second)?;
        Ok(())
    }
}
