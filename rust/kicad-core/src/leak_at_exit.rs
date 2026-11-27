// SPDX-License-Identifier: GPL-3.0-or-later
//! `include/core/leak_at_exit.h` (unit `include.core.application.pure.p001`):
//! `LEAK_AT_EXIT<T>`, a lazily created object that is never destroyed.
//!
//! The C++ heap-allocates a value-initialized `T` (`new T()`) on the first access, inside
//! `std::call_once`, and never frees it: the object outlives the wrapper, and a pointer
//! taken from `Get()` stays valid after the wrapper is destroyed. Here the first access
//! allocates one `T` on the heap and leaks it (`Vec::leak`), so the wrapper holds a
//! `&'static mut T`: dropping the wrapper neither destroys nor frees the object, and
//! [`LeakAtExit::release`] hands the surviving object to the caller when the wrapper goes
//! away, as a retained C++ pointer survives it.
//!
//! Initialization is fallible in the same two places as `new T()`: the allocation
//! (`std::bad_alloc`; `Vec::try_reserve_exact`, so it is reported, not aborted) and the
//! construction (a throwing `T()`; [`LeakAtExit::try_get_with`]). Either failure leaves
//! the wrapper uninitialized, as an exception leaves the `std::once_flag` unset, so the
//! next access tries again. [`LeakAtExit::get`] and the accessors use `T::default()` for
//! value-initialization. `Default` has no typed failure channel; a constructor that needs
//! one uses [`LeakAtExit::try_get_with`].
//!
//! A third failure, [`InitFailure::Storage`], has no C++ counterpart and no input reaches
//! it: the leaked storage, which holds the one object just pushed into it, is read back
//! with a checked access (`first_mut`), and a missing object is reported instead of
//! panicking. It too leaves the wrapper uninitialized.
//!
//! Every accessor takes `&mut self`: one owner at a time, like the other ported
//! `include/core` synchronization (`KISPINLOCK`, `SYNC_QUEUE`). `std::call_once` makes
//! concurrent first accesses create one object; exclusive access gives the same
//! guarantee. The C++ copy and move operations are deleted; the wrapper is neither `Clone`
//! nor `Copy`, and a Rust move of it does not move the object.

use std::collections::TryReserveError;

/// Why the first access could not create the object; the wrapper stays uninitialized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitFailure<E> {
    /// `new` could not allocate (`std::bad_alloc`).
    Allocation(TryReserveError),
    /// The constructor failed (threw); the allocation was released.
    Construction(E),
    /// The leaked storage did not hold the object just placed in it: a storage invariant
    /// that no input reaches, reported instead of panicking.
    Storage,
}

/// Why a value-initializing accessor ([`LeakAtExit::get`]) failed; `T::default()` cannot.
pub type GetFailure = InitFailure<core::convert::Infallible>;

/// `LEAK_AT_EXIT<T>`.
pub struct LeakAtExit<T: 'static> {
    /// `m_instance` (with `m_initFlag`): `None` until an access succeeds.
    instance: Option<&'static mut T>,
}

impl<T: Default + 'static> Default for LeakAtExit<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> LeakAtExit<T> {
    /// `LEAK_AT_EXIT()`: `m_instance( nullptr )`.
    pub const fn new() -> Self {
        Self { instance: None }
    }

    /// `Get()` with a constructor that may fail: the object, created by the first
    /// successful call. A failure leaves the wrapper uninitialized and is returned; the
    /// next call runs `init` again.
    pub fn try_get_with<E>(
        &mut self,
        init: impl FnOnce() -> Result<T, E>,
    ) -> Result<&mut T, InitFailure<E>> {
        let instance = match self.instance.take() {
            Some(instance) => instance,
            None => allocate(init)?,
        };
        Ok(&mut **self.instance.insert(instance))
    }

    /// Whether the object exists (the private `m_instance != nullptr`).
    pub fn is_created(&self) -> bool {
        self.instance.is_some()
    }

    /// Destroy the wrapper and keep its object: the C++ `T* p = &leak.Get();` retained
    /// across the wrapper's destruction. `None` when no access has created the object.
    pub fn release(self) -> Option<&'static mut T> {
        self.instance
    }
}

impl<T: Default + 'static> LeakAtExit<T> {
    /// `Get()`: the object, value-initialized by the first call of any accessor and never
    /// destroyed. Wrapper allocation errors (and the storage invariant) are returned; a
    /// fallible constructor uses [`Self::try_get_with`] instead of `Default`.
    pub fn get(&mut self) -> Result<&mut T, GetFailure> {
        self.try_get_with(|| Ok(T::default()))
    }

    /// `operator*()`: `Get()`.
    pub fn dereference(&mut self) -> Result<&mut T, GetFailure> {
        self.get()
    }

    /// `operator->()`: `&Get()`.
    pub fn arrow(&mut self) -> Result<&mut T, GetFailure> {
        self.get()
    }

    /// `operator T&()`: `Get()`.
    pub fn as_reference(&mut self) -> Result<&mut T, GetFailure> {
        self.get()
    }
}

/// `new T()`: allocate, then construct; a failing constructor releases the allocation.
fn allocate<T: 'static, E>(
    init: impl FnOnce() -> Result<T, E>,
) -> Result<&'static mut T, InitFailure<E>> {
    let mut storage = Vec::new();
    storage
        .try_reserve_exact(1)
        .map_err(InitFailure::Allocation)?;
    storage.push(init().map_err(InitFailure::Construction)?);
    // One element was pushed into an empty vector, so the leaked slice holds exactly it.
    storage.leak().first_mut().ok_or(InitFailure::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Records its destruction.
    struct Witness(Rc<Cell<u32>>, i32);

    impl Drop for Witness {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn the_object_outlives_the_wrapper() {
        let drops = Rc::new(Cell::new(0));
        let mut leak = LeakAtExit::new();
        let made = leak.try_get_with(|| Ok::<_, ()>(Witness(drops.clone(), 1)));
        assert!(made.is_ok());
        if let Ok(object) = leak.try_get_with(|| Err(())) {
            object.1 = 7;
        }
        let retained = leak.release();
        assert_eq!(drops.get(), 0);
        assert_eq!(retained.map(|object| object.1), Some(7));

        {
            // The wrapper goes out of scope; its object is never destroyed.
            let mut dropped: LeakAtExit<Witness> = LeakAtExit::new();
            assert!(dropped
                .try_get_with(|| Ok::<_, ()>(Witness(drops.clone(), 2)))
                .is_ok());
        }
        assert_eq!(drops.get(), 0);
    }

    #[test]
    fn a_failed_construction_is_retried() {
        let attempts = Cell::new(0);
        let mut leak: LeakAtExit<i32> = LeakAtExit::new();
        let init = || {
            attempts.set(attempts.get() + 1);
            if attempts.get() < 3 {
                Err("throws")
            } else {
                Ok(attempts.get() * 10)
            }
        };
        assert_eq!(
            leak.try_get_with(init).map(|v| *v),
            Err(InitFailure::Construction("throws"))
        );
        assert!(!leak.is_created());
        assert!(leak.try_get_with(init).is_err());
        assert_eq!(leak.try_get_with(init).map(|v| *v), Ok(30));
        assert_eq!(leak.try_get_with(init).map(|v| *v), Ok(30));
        assert_eq!(attempts.get(), 3);
        assert_eq!(leak.get().map(|v| *v), Ok(30));
    }
}
