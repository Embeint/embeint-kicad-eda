// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of the `UTIL::OBSERVABLE<ObserverInterface>` template in
//! `libs/core/include/core/observable.h` (unit `libs.core.include.core.application.pure.p001`).
//!
//! The template derives from `OBSERVABLE_BASE` ([`crate::observable`]); its subscription
//! list lives in an [`ObservableArena`]. Observers are identified by [`ObserverId`]
//! (the reference's `void*`, `None` being `nullptr`); the host maps an id to its typed
//! observer object and performs the member-function call, which is the
//! `( typed_ptr->*Ptr )( std::forward<Args2>( aArgs )... )` expression of the reference.
//!
//! `Notify` and `NotifyIgnore` take a [`NotifyHost`], because an observer may change
//! anything reachable from the host, including the notifying observable itself, while
//! the loop runs. The loop ranges over the list `impl_` named when it started, with
//! the end fixed at that time, exactly as the reference's range-for does; it re-reads
//! the entry at each position. A callback that resizes that list (an append, an erase,
//! a compacting `collect()`) or destroys it leaves the reference reading through an
//! invalidated iterator: that is refused with [`NotifyError::ListInvalidated`] before
//! the loop reads again. An observer failure (`Err`) is the reference's exception: both
//! templates' `catch( ... )` blocks call `leave_iteration()` and rethrow. On normal exit
//! `Notify` calls `leave_iteration()` and `NotifyIgnore` does not (the reference leaves
//! that iteration open; see `port/upstream-issues/core-observable-unmanaged-release.md`).

use core::marker::PhantomData;

use crate::observable::{
    ImplRef, Link, ObservableArena, ObservableBase, ObservableError, ObserverId, ObserverPtr,
};

/// `UTIL::OBSERVABLE<ObserverInterface>`: an `OBSERVABLE_BASE` whose observers implement
/// `ObserverInterface` (the type parameter, a marker here).
#[derive(Debug)]
#[must_use = "an Observable may hold an ImplRef; release it with drop_observable"]
pub struct Observable<T: ?Sized> {
    base: ObservableBase,
    interface: PhantomData<fn(&T)>,
}

impl<T: ?Sized> Default for Observable<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The host side of a notification: storage for the arena and the notifying observable,
/// and the observer call itself.
pub trait NotifyHost<T: ?Sized> {
    /// What an observer call can fail with: the reference's exception.
    type Error;

    /// The arena and the notifying observable (`this`). `None` once a callback has
    /// destroyed that observable; the host must not hand back a different object.
    fn parts(&mut self) -> (&mut ObservableArena, Option<&mut Observable<T>>);

    /// Call the notification member of the observer `observer` with the notification's
    /// arguments.
    ///
    /// # Errors
    /// An error is an exception thrown by the observer.
    fn call(&mut self, observer: ObserverId) -> Result<(), Self::Error>;
}

/// Why a notification stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyError<E> {
    /// An observer failed. As in the reference's `catch( ... )`, `leave_iteration()` ran
    /// on the notifying observable before the error was returned.
    Observer(E),
    /// An observer failed after the notifying observable was destroyed: the reference's
    /// `catch( ... )` then calls `leave_iteration()` on a destroyed object (undefined).
    /// Nothing more was done.
    UnwoundDestroyed(E),
    /// A base operation failed (foreign or stale handle, allocation).
    Observable(ObservableError),
    /// A callback appended to, erased from, compacted or destroyed the list the loop
    /// is reading; the reference's next iterator use is undefined.
    ListInvalidated,
    /// The notifying observable did not exist when the template needed it: at the
    /// start, or (`Notify` only) for the `leave_iteration()` after its loop, which the
    /// reference performs on a destroyed object (undefined).
    Destroyed,
}

impl<E> From<ObservableError> for NotifyError<E> {
    fn from(error: ObservableError) -> Self {
        Self::Observable(error)
    }
}

impl<T: ?Sized> Observable<T> {
    /// `OBSERVABLE()`: an empty, non-shared (null) subscription list.
    pub fn new() -> Self {
        Self {
            base: ObservableBase::new(),
            interface: PhantomData,
        }
    }

    /// `OBSERVABLE( OBSERVABLE& aInherit )`: share `inherit`'s subscription list, which
    /// becomes shared (a shared list is allocated for `inherit` if it has none).
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`], [`ObservableError::StaleHandle`] or
    /// [`ObservableError::Allocation`].
    pub fn inherit(
        arena: &mut ObservableArena,
        inherit: &mut Observable<T>,
    ) -> Result<Self, ObservableError> {
        Ok(Self {
            base: arena.copy_observable(&mut inherit.base)?,
            interface: PhantomData,
        })
    }

    /// The `OBSERVABLE_BASE` part.
    pub fn base(&self) -> &ObservableBase {
        &self.base
    }

    /// The `OBSERVABLE_BASE` part, for its protected members (`enter_iteration`, ...).
    pub fn base_mut(&mut self) -> &mut ObservableBase {
        &mut self.base
    }

    /// The `OBSERVABLE_BASE` part, for destruction with
    /// [`ObservableArena::drop_observable`] (`~OBSERVABLE`, which adds nothing).
    pub fn into_base(self) -> ObservableBase {
        self.base
    }

    /// `SubscribeUnmanaged`: append `observer` without a link.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`], [`ObservableError::StaleHandle`] or
    /// [`ObservableError::Allocation`].
    pub fn subscribe_unmanaged(
        &mut self,
        arena: &mut ObservableArena,
        observer: ObserverPtr,
    ) -> Result<(), ObservableError> {
        arena.add_observer(&mut self.base, observer)
    }

    /// `Subscribe`: append `observer` and return `LINK( impl_, observer )`, which holds
    /// a counted reference to the list and removes the observer when reset or dropped.
    ///
    /// # Errors
    /// [`ObservableError::ForeignHandle`], [`ObservableError::StaleHandle`] or
    /// [`ObservableError::Allocation`].
    pub fn subscribe(
        &mut self,
        arena: &mut ObservableArena,
        observer: ObserverPtr,
    ) -> Result<Link, ObservableError> {
        arena.add_observer(&mut self.base, observer)?;
        let token = arena.share_impl(&self.base)?;
        Ok(Link::with_token(token, observer))
    }

    /// `Unsubscribe`: remove the first `observer` entry (nulled while the list is
    /// iterating). Callable from inside a notification.
    ///
    /// # Errors
    /// [`ObservableError::NoImpl`] when there is no list (the reference dereferences a
    /// null `impl_`), [`ObservableError::ForeignHandle`] or
    /// [`ObservableError::StaleHandle`].
    pub fn unsubscribe(
        &mut self,
        arena: &mut ObservableArena,
        observer: ObserverPtr,
    ) -> Result<(), ObservableError> {
        arena.remove_observer(&self.base, observer)
    }
}

/// `Notify( Ptr, aArgs... )`: call every non-null observer of the list.
///
/// # Errors
/// See [`NotifyError`].
pub fn notify<T: ?Sized, H: NotifyHost<T>>(host: &mut H) -> Result<(), NotifyError<H::Error>> {
    notify_loop(host, None, true)
}

/// `NotifyIgnore( Ptr, aIgnore, aArgs... )`: call every non-null observer except
/// `ignore` (`None` ignores nothing). Unlike [`notify`], a normal exit leaves the
/// iteration count raised.
///
/// # Errors
/// See [`NotifyError`].
pub fn notify_ignore<T: ?Sized, H: NotifyHost<T>>(
    host: &mut H,
    ignore: ObserverPtr,
) -> Result<(), NotifyError<H::Error>> {
    notify_loop(host, Some(ignore), false)
}

/// The shared body. `ignore` is `NotifyIgnore`'s `aIgnore`; `leave_after` is true for
/// `Notify`, whose normal exit calls `leave_iteration()`.
fn notify_loop<T: ?Sized, H: NotifyHost<T>>(
    host: &mut H,
    ignore: Option<ObserverPtr>,
    leave_after: bool,
) -> Result<(), NotifyError<H::Error>> {
    let (arena, this) = host.parts();
    let this = this.ok_or(NotifyError::Destroyed)?;

    // if( impl_ )
    let Some(id) = this.base.impl_ref().map(ImplRef::id) else {
        return Ok(());
    };

    arena.enter_iteration(&this.base)?;

    // for( auto* void_ptr : impl_->observers_ ): the range and its end are fixed here.
    let end = arena.observers(id)?.len();
    let revision = arena.structure_revision(id)?;

    for position in 0..end {
        let entry = {
            let (arena, _) = host.parts();
            arena.observers(id)?.get(position).copied().flatten()
        };

        // if( void_ptr ) / if( void_ptr && void_ptr != aIgnore )
        let Some(observer) = entry else {
            continue;
        };

        if ignore == Some(Some(observer)) {
            continue;
        }

        if let Err(error) = host.call(observer) {
            // catch( ... ) { leave_iteration(); throw; }
            let (arena, this) = host.parts();
            return match this {
                Some(this) => {
                    arena.leave_iteration(&mut this.base)?;
                    Err(NotifyError::Observer(error))
                }
                None => Err(NotifyError::UnwoundDestroyed(error)),
            };
        }

        // The loop's next step increments and compares iterators into this list.
        let (arena, _) = host.parts();

        if !arena.is_live(id) || arena.structure_revision(id)? != revision {
            return Err(NotifyError::ListInvalidated);
        }
    }

    if leave_after {
        let (arena, this) = host.parts();
        let this = this.ok_or(NotifyError::Destroyed)?;
        arena.leave_iteration(&mut this.base)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observable::ArenaId;

    struct Iface;

    const A: ObserverId = ObserverId(1);
    const B: ObserverId = ObserverId(2);

    /// What an observer does when called.
    #[derive(Clone, Copy, PartialEq)]
    enum Action {
        Nothing,
        Fail,
        UnsubscribeSelf,
        SubscribeB,
        DropThis,
    }

    struct Host {
        arena: ObservableArena,
        this: Option<Observable<Iface>>,
        calls: Vec<ObserverId>,
        action: Action,
    }

    impl NotifyHost<Iface> for Host {
        type Error = &'static str;

        fn parts(&mut self) -> (&mut ObservableArena, Option<&mut Observable<Iface>>) {
            (&mut self.arena, self.this.as_mut())
        }

        fn call(&mut self, observer: ObserverId) -> Result<(), &'static str> {
            self.calls.push(observer);
            match self.action {
                Action::Nothing => Ok(()),
                Action::Fail => Err("thrown"),
                Action::UnsubscribeSelf => match self.this.as_mut() {
                    Some(this) => this
                        .unsubscribe(&mut self.arena, Some(observer))
                        .map_err(|_| "unsubscribe"),
                    None => Err("gone"),
                },
                Action::SubscribeB => match self.this.as_mut() {
                    Some(this) => this
                        .subscribe_unmanaged(&mut self.arena, Some(B))
                        .map_err(|_| "subscribe"),
                    None => Err("gone"),
                },
                Action::DropThis => match self.this.take() {
                    Some(this) => self
                        .arena
                        .drop_observable(this.into_base())
                        .map_err(|_| "drop"),
                    None => Ok(()),
                },
            }
        }
    }

    fn host(action: Action) -> Result<(Host, Link), ObservableError> {
        let mut arena = ObservableArena::new(ArenaId(0));
        let mut this = Observable::new();
        let link = this.subscribe(&mut arena, Some(A))?;
        this.subscribe_unmanaged(&mut arena, None)?;
        this.subscribe_unmanaged(&mut arena, Some(B))?;
        Ok((
            Host {
                arena,
                this: Some(this),
                calls: Vec::new(),
                action,
            },
            link,
        ))
    }

    fn list(host: &Host) -> Vec<ObserverPtr> {
        host.this
            .as_ref()
            .and_then(|this| this.base().impl_ref())
            .and_then(|r| host.arena.observers(r.id()).ok())
            .map(<[ObserverPtr]>::to_vec)
            .unwrap_or_default()
    }

    fn count(host: &Host) -> Option<u32> {
        host.this
            .as_ref()
            .and_then(|this| this.base().impl_ref())
            .and_then(|r| host.arena.iteration_count(r.id()).ok())
    }

    #[test]
    fn notify_skips_null_and_balances() -> Result<(), ObservableError> {
        let (mut h, link) = host(Action::Nothing)?;
        assert_eq!(notify(&mut h), Ok(()));
        assert_eq!(h.calls, [A, B]);
        assert_eq!(count(&h), Some(0));
        h.arena.drop_link(link)?;
        Ok(())
    }

    #[test]
    fn notify_ignore_skips_and_stays_raised() -> Result<(), ObservableError> {
        let (mut h, link) = host(Action::Nothing)?;
        assert_eq!(notify_ignore(&mut h, Some(A)), Ok(()));
        assert_eq!(h.calls, [B]);
        assert_eq!(count(&h), Some(1));
        h.calls.clear();
        assert_eq!(notify_ignore(&mut h, None), Ok(()));
        assert_eq!(h.calls, [A, B]);
        assert_eq!(count(&h), Some(2));
        h.arena.drop_link(link)?;
        Ok(())
    }

    #[test]
    fn failure_leaves_iteration_in_both_templates() -> Result<(), ObservableError> {
        let (mut h, link) = host(Action::Fail)?;
        assert_eq!(notify(&mut h), Err(NotifyError::Observer("thrown")));
        assert_eq!(h.calls, [A]);
        assert_eq!(count(&h), Some(0));
        assert_eq!(
            notify_ignore(&mut h, Some(A)),
            Err(NotifyError::Observer("thrown"))
        );
        assert_eq!(count(&h), Some(0));
        h.arena.drop_link(link)?;
        Ok(())
    }

    #[test]
    fn self_removal_is_nulled_then_collected() -> Result<(), ObservableError> {
        let (mut h, link) = host(Action::UnsubscribeSelf)?;
        assert_eq!(notify(&mut h), Ok(()));
        assert_eq!(h.calls, [A, B]);
        assert_eq!(list(&h), []);
        h.arena.drop_link(link)?;
        Ok(())
    }

    #[test]
    fn append_during_loop_is_refused() -> Result<(), ObservableError> {
        let (mut h, link) = host(Action::SubscribeB)?;
        assert_eq!(notify(&mut h), Err(NotifyError::ListInvalidated));
        assert_eq!(h.calls, [A]);
        h.arena.drop_link(link)?;
        Ok(())
    }

    #[test]
    fn destroying_this_differs_between_templates() -> Result<(), ObservableError> {
        let (mut h, link) = host(Action::DropThis)?;
        // The link keeps the list alive, so NotifyIgnore reads on and returns.
        assert_eq!(notify_ignore(&mut h, None), Ok(()));
        assert_eq!(h.calls, [A, B]);
        h.arena.drop_link(link)?;

        let (mut h, link) = host(Action::DropThis)?;
        assert_eq!(notify(&mut h), Err(NotifyError::Destroyed));
        h.arena.drop_link(link)?;
        Ok(())
    }

    #[test]
    fn no_list_means_no_calls() {
        let mut h = Host {
            arena: ObservableArena::new(ArenaId(0)),
            this: Some(Observable::new()),
            calls: Vec::new(),
            action: Action::Fail,
        };
        assert_eq!(notify(&mut h), Ok(()));
        assert_eq!(notify_ignore(&mut h, None), Ok(()));
        assert!(h.calls.is_empty());
    }
}
