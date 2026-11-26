// SPDX-License-Identifier: GPL-3.0-or-later
//! `include/core/inplace_function.h` (unit `include.core.application.pure.p001`):
//! `INPLACE_FUNCTION<R( Args... ), Capacity>`, a copyable, type-erased callable wrapper.
//!
//! The signature `R( Args... )` is the argument tuple `A` and result `R`; `Capacity`
//! keeps its default of four pointers ([`DEFAULT_CAPACITY`]). The C++ compile-time
//! requirements are compile-time here too: storing a callable larger than `CAPACITY`
//! bytes, or aligned more strictly than `std::max_align_t`, fails to build
//! ([`InplaceFunction::from_callable`]). Rust moves cannot throw, so the
//! nothrow-move requirement always holds; the `COPY` operation needs `Clone`, as the C++
//! needs a copy constructor.
//!
//! Storage differs: the C++ constructs the callable in an inline buffer, which safe Rust
//! cannot type-erase without `unsafe`; the callable is boxed instead. That is not
//! observable through any operation's result, but it allocates (except for zero-sized
//! callables) where the C++ never does, and an allocation failure aborts the process.
//! The lifetime operations are the C++ ones: `DESTROY` drops the callable once, `COPY`
//! clones it, `MOVE` relocates it and leaves the source empty.

use core::marker::PhantomData;

/// `4 * sizeof( void* )`, the default `Capacity`.
pub const DEFAULT_CAPACITY: usize = 4 * core::mem::size_of::<*const ()>();

/// `alignof( std::max_align_t )` on the reference platforms (x86-64 and wasm32).
pub const MAX_ALIGN: usize = 16;

/// `std::bad_function_call`: invoking an empty wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadFunctionCall;

/// A stored target: `m_invoke` (call) and `m_manage` (`COPY`; `DESTROY` is `Drop`, `MOVE`
/// a relocation of the box) for one concrete callable type.
pub trait Target<'a, A, R> {
    /// `m_invoke( &m_storage, aArgs... )`.
    fn invoke(&self, args: A) -> R;

    /// `m_manage( OP::COPY, dst, src )`: a copy of the callable.
    fn copy(&self) -> Box<dyn Target<'a, A, R> + 'a>;
}

macro_rules! targets {
    ($($arg:ident),*) => {
        impl<'a, F, R $(, $arg)*> Target<'a, ($($arg,)*), R> for F
        where
            F: Fn($($arg),*) -> R + Clone + 'a,
        {
            #[allow(non_snake_case)]
            fn invoke(&self, args: ($($arg,)*)) -> R {
                let ($($arg,)*) = args;
                self($($arg),*)
            }

            fn copy(&self) -> Box<dyn Target<'a, ($($arg,)*), R> + 'a> {
                Box::new(self.clone())
            }
        }
    };
}
targets!();
targets!(A1);
targets!(A1, A2);
targets!(A1, A2, A3);
targets!(A1, A2, A3, A4);

/// The `static_assert`s of `assign()`, evaluated when a callable type is stored.
struct Requirements<F, const CAPACITY: usize>(PhantomData<F>);

impl<F, const CAPACITY: usize> Requirements<F, CAPACITY> {
    const HOLD: () = {
        assert!(
            core::mem::size_of::<F>() <= CAPACITY,
            "callable too large for INPLACE_FUNCTION; raise its Capacity argument"
        );
        assert!(
            core::mem::align_of::<F>() <= MAX_ALIGN,
            "callable over-aligned"
        );
    };
}

/// `INPLACE_FUNCTION<R( A... ), CAPACITY>`.
pub struct InplaceFunction<'a, A, R, const CAPACITY: usize = DEFAULT_CAPACITY> {
    /// The callable; `None` is `m_invoke == nullptr` (and `m_manage == nullptr`).
    target: Option<Box<dyn Target<'a, A, R> + 'a>>,
}

impl<'a, A, R, const CAPACITY: usize> Default for InplaceFunction<'a, A, R, CAPACITY> {
    /// `INPLACE_FUNCTION() = default`: empty.
    fn default() -> Self {
        Self { target: None }
    }
}

impl<'a, A, R, const CAPACITY: usize> InplaceFunction<'a, A, R, CAPACITY> {
    /// `INPLACE_FUNCTION()`: empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// `INPLACE_FUNCTION( F&& aCallable )`: `assign( aCallable )`.
    pub fn from_callable<F>(callable: F) -> Self
    where
        F: Target<'a, A, R> + 'a,
    {
        let mut function = Self::new();
        function.assign(callable);
        function
    }

    /// `INPLACE_FUNCTION( INPLACE_FUNCTION&& aOther )`: `moveFrom( aOther )`.
    pub fn take(other: &mut Self) -> Self {
        let mut function = Self::new();
        function.move_from(other);
        function
    }

    /// `operator=( const INPLACE_FUNCTION& aOther )` for another object: the copy is built
    /// before the old target is destroyed, then moved in. Assigning an object to itself
    /// (`this == &aOther`, which does nothing) cannot be expressed with Rust's exclusive
    /// borrows.
    pub fn copy_assign(&mut self, other: &Self) {
        let mut tmp = other.clone();
        self.reset();
        self.move_from(&mut tmp);
    }

    /// `operator=( INPLACE_FUNCTION&& aOther )` for another object: destroy the target,
    /// then take `aOther`'s, leaving it empty. Self-assignment cannot be expressed, as for
    /// [`Self::copy_assign`].
    pub fn move_assign(&mut self, other: &mut Self) {
        self.reset();
        self.move_from(other);
    }

    /// `operator=( F&& aCallable )`: build a temporary from the callable, destroy the old
    /// target, then move the temporary in.
    pub fn assign_callable<F>(&mut self, callable: F)
    where
        F: Target<'a, A, R> + 'a,
    {
        let mut tmp = Self::from_callable(callable);
        self.reset();
        self.move_from(&mut tmp);
    }

    /// `explicit operator bool()`: `m_invoke != nullptr`.
    pub fn is_set(&self) -> bool {
        self.target.is_some()
    }

    /// `operator()( Args... aArgs )`: `std::bad_function_call` when empty.
    pub fn call(&self, args: A) -> Result<R, BadFunctionCall> {
        match &self.target {
            None => Err(BadFunctionCall),
            Some(target) => Ok(target.invoke(args)),
        }
    }

    /// `assign( F&& aCallable )`: store the callable (the `static_assert`s are checked when
    /// this is instantiated). Called on an empty wrapper, as in the C++.
    fn assign<F>(&mut self, callable: F)
    where
        F: Target<'a, A, R> + 'a,
    {
        #[allow(clippy::let_unit_value)]
        let () = Requirements::<F, CAPACITY>::HOLD;
        self.target = Some(Box::new(callable));
    }

    /// `reset()`: destroy the target, if any, and become empty.
    fn reset(&mut self) {
        if self.target.is_some() {
            self.target = None;
        }
    }

    /// `moveFrom( aOther )`: take `aOther`'s target, if any, leaving it empty.
    fn move_from(&mut self, other: &mut Self) {
        if let Some(target) = other.target.take() {
            self.target = Some(target);
        }
    }

    /// `copyFrom( aOther )`: copy `aOther`'s target, if any.
    fn copy_from(&mut self, other: &Self) {
        if let Some(target) = &other.target {
            self.target = Some(target.copy());
        }
    }
}

impl<'a, A, R, const CAPACITY: usize> Clone for InplaceFunction<'a, A, R, CAPACITY> {
    /// `INPLACE_FUNCTION( const INPLACE_FUNCTION& aOther )`: `copyFrom( aOther )`.
    fn clone(&self) -> Self {
        let mut function = Self::new();
        function.copy_from(self);
        function
    }
}
