// SPDX-License-Identifier: GPL-3.0-or-later
//! `include/core/leak_at_exit.h` (unit `include.core.application.pure.p001`):
//! `LEAK_AT_EXIT<T>`, a lazily created object that is never destroyed.
//!
//! The C++ heap-allocates a value-initialized `T` (`new T()`) on the first access, inside
//! `std::call_once`, and never frees it. Here the object is created by the first access
//! and held in [`ManuallyDrop`], so dropping the wrapper never runs `T`'s destructor
//! either: whatever the object owns stays alive. `T::default()` is value-initialization
//! for every type the port uses (zero for integers, empty containers, a null pointer).
//!
//! Every accessor takes `&mut self`: one owner at a time, like the other ported
//! `include/core` synchronization (`KISPINLOCK`, `SYNC_QUEUE`). `std::call_once` makes
//! concurrent first accesses create one object; exclusive access gives the same
//! guarantee. A value-initialization that throws (`std::bad_alloc`, or `T()` throwing)
//! leaves the flag unset in C++ so a later access retries; `T::default()` cannot report
//! failure, so that path has no Rust counterpart.
//!
//! The C++ copy and move operations are deleted; the wrapper is neither `Clone` nor
//! `Copy`. A Rust move is a relocation that no C++ program can observe on a non-movable
//! object, and the leaked object moves with it.

use core::mem::ManuallyDrop;

/// `LEAK_AT_EXIT<T>`.
pub struct LeakAtExit<T> {
    /// `m_instance` (with `m_initFlag`): `None` until the first access.
    instance: Option<ManuallyDrop<T>>,
}

impl<T: Default> Default for LeakAtExit<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Default> LeakAtExit<T> {
    /// `LEAK_AT_EXIT()`: `m_instance( nullptr )`.
    pub const fn new() -> Self {
        Self { instance: None }
    }

    /// `Get()`: the object, created by the first call of any accessor and never destroyed.
    pub fn get(&mut self) -> &mut T {
        self.instance
            .get_or_insert_with(|| ManuallyDrop::new(T::default()))
    }

    /// `operator*()`: `Get()`.
    pub fn dereference(&mut self) -> &mut T {
        self.get()
    }

    /// `operator->()`: `&Get()`.
    pub fn arrow(&mut self) -> &mut T {
        self.get()
    }

    /// `operator T&()`: `Get()`.
    pub fn as_reference(&mut self) -> &mut T {
        self.get()
    }

    /// Whether the object exists (the private `m_instance != nullptr`).
    pub fn is_created(&self) -> bool {
        self.instance.is_some()
    }
}
