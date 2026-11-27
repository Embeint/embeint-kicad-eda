// SPDX-License-Identifier: GPL-3.0-or-later
//! `include/core/inplace_function.h` (unit `include.core.application.pure.p001`):
//! `INPLACE_FUNCTION<R( Args... ), Capacity>`, a copyable, type-erased callable wrapper that
//! never allocates.
//!
//! The signature `R( Args... )` is the argument tuple `A` and result `R`; `Capacity`
//! keeps its default of four pointers ([`DEFAULT_CAPACITY`]).
//!
//! **Storage.** As in C++, the callable is constructed inside the wrapper: `CAPACITY` bytes
//! of payload at `alignof( std::max_align_t )` ([`MAX_ALIGN`]) in a
//! `stack_dst::Value<dyn _, MaxAlignBuf<CAPACITY>>` (the workspace's patched `stack_dst`;
//! `rust/vendor/stack_dst/EMBEINT-PATCH.md`). The `dyn` metadata, which stands for the C++
//! `m_invoke` / `m_manage` pointers, has its own word after the payload and never takes
//! payload space. Wrapper storage never allocates or falls back to the heap. A callable's
//! own special members can allocate; fallible copies report [`CopyFailure`]. The C++
//! `static_assert`s are separate compile-time assertions on the callable type
//! ([`InplaceFunction::from_callable`]): larger than `CAPACITY` bytes, or aligned more
//! strictly than [`MAX_ALIGN`], fails to build. They check the Rust type's size and
//! alignment; they say nothing about how a C++ compiler lays out a lambda.
//!
//! **Lifecycle.** `m_manage` performs the callable's own special members, and a program can
//! observe them. [`Callable`] makes them explicit: `move_construct` is the move constructor
//! (`OP::MOVE`), after which the wrapper drops the moved-from source as `moveFrom`'s
//! `aOther.reset()` destroys it; `copy_construct` is the copy constructor (`OP::COPY`) and
//! may fail, as a C++ copy constructor may throw; `Drop` is `OP::DESTROY`. Rust closures
//! have no move constructor (a Rust move is a relocation no program can observe), so they
//! are relocated and their copy is `Clone`, which has no typed failure channel.
//!
//! **Failure.** A failing copy is reported as [`CopyFailure`] and leaves everything as the
//! C++ leaves it when the copy constructor throws: the copy constructor builds nothing, and
//! copy assignment ([`InplaceFunction::try_copy_assign`]) keeps the old target (the C++
//! builds the copy before it resets). Moves cannot fail: the C++ requires nothrow moves.

use core::marker::PhantomData;
use stack_dst::embeint_inline::MaxAlignBuf;
use stack_dst::Value;

/// `4 * sizeof( void* )`, the default `Capacity`.
pub const DEFAULT_CAPACITY: usize = 4 * core::mem::size_of::<*const ()>();

/// `alignof( std::max_align_t )` on the reference platforms (x86-64 and wasm32).
pub const MAX_ALIGN: usize = 16;

/// `std::bad_function_call`: invoking an empty wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadFunctionCall;

/// The failure of a callable's copy constructor (the exception it throws in C++).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyFailure;

/// A callable `INPLACE_FUNCTION` can store, with the special members its `m_manage` calls.
///
/// Every `Fn` closure that is `Clone` implements it (relocated on move, cloned on copy). A
/// type with observable lifecycle implements it directly; its `Drop` is the destructor.
pub trait Callable<A, R>: Sized {
    /// `m_invoke`: `( *static_cast<const DECAYED*>( aStorage ) )( aArgs... )`.
    fn invoke(&self, args: A) -> R;

    /// `OP::MOVE`, `DECAYED( std::move( src ) )`: the new object, leaving `self` in its
    /// moved-from state for the wrapper to destroy. `None` when the type has no move
    /// constructor of its own: the wrapper then relocates it.
    fn move_construct(&mut self) -> Option<Self>;

    /// `OP::COPY`, `DECAYED( src )`: the copy, or the copy constructor's failure.
    fn copy_construct(&self) -> Result<Self, CopyFailure>;
}

macro_rules! closures {
    ($($arg:ident),*) => {
        impl<F, R $(, $arg)*> Callable<($($arg,)*), R> for F
        where
            F: Fn($($arg),*) -> R + Clone,
        {
            #[allow(non_snake_case)]
            fn invoke(&self, args: ($($arg,)*)) -> R {
                let ($($arg,)*) = args;
                self($($arg),*)
            }

            fn move_construct(&mut self) -> Option<Self> {
                None
            }

            fn copy_construct(&self) -> Result<Self, CopyFailure> {
                Ok(self.clone())
            }
        }
    };
}
closures!();
closures!(A1);
closures!(A1, A2);
closures!(A1, A2, A3);
closures!(A1, A2, A3, A4);

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

/// The inline storage: the payload buffer and the erased operations (`m_invoke`, `m_manage`).
type Storage<'a, A, R, const CAPACITY: usize> =
    Value<dyn Stored<'a, A, R, CAPACITY> + 'a, MaxAlignBuf<CAPACITY>>;

/// The type-erased operations on a stored callable.
trait Stored<'a, A, R, const CAPACITY: usize> {
    fn invoke(&self, args: A) -> R;

    /// `OP::MOVE` into new storage; `None`: relocate this storage instead.
    fn move_out(&mut self) -> Option<Storage<'a, A, R, CAPACITY>>;

    /// `OP::COPY` into new storage.
    fn copy_out(&self) -> Result<Storage<'a, A, R, CAPACITY>, CopyFailure>;
}

impl<'a, F, A, R, const CAPACITY: usize> Stored<'a, A, R, CAPACITY> for F
where
    F: Callable<A, R> + 'a,
{
    fn invoke(&self, args: A) -> R {
        Callable::invoke(self, args)
    }

    fn move_out(&mut self) -> Option<Storage<'a, A, R, CAPACITY>> {
        self.move_construct().map(place)
    }

    fn copy_out(&self) -> Result<Storage<'a, A, R, CAPACITY>, CopyFailure> {
        self.copy_construct().map(place)
    }
}

/// The pointer coercion `Value` checks: the callable itself, as its erased operations.
fn erase<'r, 'a, F, A, R, const CAPACITY: usize>(
    callable: &'r F,
) -> &'r (dyn Stored<'a, A, R, CAPACITY> + 'a)
where
    F: Callable<A, R> + 'a,
{
    callable
}

/// `::new( &m_storage ) DECAYED( ... )`: place an already constructed callable in inline
/// storage. The size and alignment assertions are checked when this is instantiated.
fn place<'a, F, A, R, const CAPACITY: usize>(callable: F) -> Storage<'a, A, R, CAPACITY>
where
    F: Callable<A, R> + 'a,
{
    #[allow(clippy::let_unit_value)]
    let () = Requirements::<F, CAPACITY>::HOLD;

    match Value::new_stable(callable, erase::<F, A, R, CAPACITY>) {
        Ok(storage) => storage,
        // `Requirements` proved at compile time that `F` fits the payload and its alignment,
        // which is all `new_stable` checks.
        Err(_) => unreachable!("INPLACE_FUNCTION payload checked at compile time"),
    }
}

/// `INPLACE_FUNCTION<R( A... ), CAPACITY>`.
pub struct InplaceFunction<'a, A, R, const CAPACITY: usize = DEFAULT_CAPACITY> {
    /// `m_storage` with `m_invoke` and `m_manage`; `None` is `m_invoke == nullptr`.
    target: Option<Storage<'a, A, R, CAPACITY>>,
}

impl<A, R, const CAPACITY: usize> Default for InplaceFunction<'_, A, R, CAPACITY> {
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

    /// `INPLACE_FUNCTION( F&& aCallable )` with an rvalue: `assign` move-constructs the
    /// callable into the storage. The moved-from argument is destroyed when the
    /// construction is complete, as the C++ temporary is at the end of the full-expression.
    pub fn from_callable<F>(callable: F) -> Self
    where
        F: Callable<A, R> + 'a,
    {
        let (target, _moved_from) = construct(callable);
        Self {
            target: Some(target),
        }
    }

    /// `INPLACE_FUNCTION( F&& aCallable )` with an lvalue (`F` deduced as `F&`): `assign`
    /// copy-constructs the callable; a failing copy constructs nothing.
    pub fn try_from_copy<F>(callable: &F) -> Result<Self, CopyFailure>
    where
        F: Callable<A, R> + 'a,
    {
        Ok(Self {
            target: Some(place(callable.copy_construct()?)),
        })
    }

    /// `INPLACE_FUNCTION( const INPLACE_FUNCTION& aOther )`: `copyFrom( aOther )`. A
    /// failing copy constructs nothing.
    pub fn try_clone(&self) -> Result<Self, CopyFailure> {
        let mut function = Self::new();
        function.copy_from(self)?;
        Ok(function)
    }

    /// `INPLACE_FUNCTION( INPLACE_FUNCTION&& aOther )`: `moveFrom( aOther )`.
    pub fn take(other: &mut Self) -> Self {
        let mut function = Self::new();
        function.move_from(other);
        function
    }

    /// `operator=( const INPLACE_FUNCTION& aOther )` for another object: the copy is built
    /// before the old target is reset, then moved in. A failing copy leaves `self`
    /// unchanged.
    pub fn try_copy_assign(&mut self, other: &Self) -> Result<(), CopyFailure> {
        let mut copy = other.try_clone()?;
        self.reset();
        self.move_from(&mut copy);
        Ok(())
    }

    /// `operator=( const INPLACE_FUNCTION& )` with `this == &aOther`: nothing happens.
    /// (Rust's exclusive borrow cannot name `self` as the source of
    /// [`Self::try_copy_assign`]; this is that call.)
    pub fn try_copy_assign_self(&mut self) -> Result<(), CopyFailure> {
        Ok(())
    }

    /// `operator=( INPLACE_FUNCTION&& aOther )` for another object: reset, then take
    /// `aOther`'s target, leaving it empty.
    pub fn move_assign(&mut self, other: &mut Self) {
        self.reset();
        self.move_from(other);
    }

    /// `operator=( INPLACE_FUNCTION&& )` with `this == &aOther`: nothing happens.
    pub fn move_assign_self(&mut self) {}

    /// `operator=( F&& aCallable )` with an rvalue: build a temporary wrapper from the
    /// callable, reset, then move the temporary in. The moved-from argument is destroyed
    /// last, as the C++ temporary is.
    pub fn assign_callable<F>(&mut self, callable: F)
    where
        F: Callable<A, R> + 'a,
    {
        let (target, moved_from) = construct(callable);
        let mut temporary = Self {
            target: Some(target),
        };
        self.reset();
        self.move_from(&mut temporary);
        drop(temporary);
        drop(moved_from);
    }

    /// `operator=( F&& aCallable )` with an lvalue: the temporary wrapper copies the
    /// callable first, so a failing copy leaves `self` unchanged.
    pub fn try_assign_copy<F>(&mut self, callable: &F) -> Result<(), CopyFailure>
    where
        F: Callable<A, R> + 'a,
    {
        let mut temporary = Self::try_from_copy(callable)?;
        self.reset();
        self.move_from(&mut temporary);
        Ok(())
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

    /// `reset()`: destroy the target, if any, and become empty.
    fn reset(&mut self) {
        if self.target.is_some() {
            self.target = None;
        }
    }

    /// `moveFrom( aOther )` into an empty wrapper: `OP::MOVE`, then `aOther.reset()`
    /// destroys the moved-from source. A relocated callable has no source left.
    fn move_from(&mut self, other: &mut Self) {
        if let Some(mut source) = other.target.take() {
            match source.move_out() {
                Some(moved) => {
                    self.target = Some(moved);
                    drop(source);
                }
                None => self.target = Some(source),
            }
        }
    }

    /// `copyFrom( aOther )` into an empty wrapper: `OP::COPY`, if `aOther` has a target.
    fn copy_from(&mut self, other: &Self) -> Result<(), CopyFailure> {
        if let Some(source) = &other.target {
            self.target = Some(source.copy_out()?);
        }
        Ok(())
    }
}

/// `assign( std::forward<F>( aCallable ) )` from an rvalue: the stored object and, when the
/// callable has a move constructor, the moved-from argument the caller destroys later.
fn construct<'a, F, A, R, const CAPACITY: usize>(
    mut callable: F,
) -> (Storage<'a, A, R, CAPACITY>, Option<F>)
where
    F: Callable<A, R> + 'a,
{
    match callable.move_construct() {
        Some(moved) => (place(moved), Some(callable)),
        None => (place(callable), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// A callable that reports where it is stored.
    #[derive(Clone)]
    struct Whereabouts([u64; 2]);

    impl Callable<(), usize> for Whereabouts {
        fn invoke(&self, _: ()) -> usize {
            debug_assert_eq!(self.0, [0; 2]);
            self as *const Self as usize
        }

        fn move_construct(&mut self) -> Option<Self> {
            None
        }

        fn copy_construct(&self) -> Result<Self, CopyFailure> {
            Ok(self.clone())
        }
    }

    #[test]
    fn callables_are_stored_inside_the_wrapper() {
        let wide = [1i64, 2, 3, 4];
        let function: InplaceFunction<'_, (i64,), i64> =
            InplaceFunction::from_callable(move |x: i64| wide.iter().sum::<i64>() + x);
        assert_eq!(function.call((5,)), Ok(15));
        assert_eq!(
            core::mem::align_of::<InplaceFunction<'_, (), usize>>(),
            MAX_ALIGN
        );

        let mut functions: [InplaceFunction<'_, (), usize>; 2] = Default::default();
        functions[0] = InplaceFunction::from_callable(Whereabouts([0; 2]));
        let copy = functions[0].try_clone();
        assert!(copy.is_ok());
        if let Ok(copy) = copy {
            functions[1] = copy;
        }
        for function in &functions {
            let start = function as *const InplaceFunction<'_, (), usize> as usize;
            let end = start + core::mem::size_of::<InplaceFunction<'_, (), usize>>();
            let at = function.call(());
            assert!(at.is_ok_and(|at| (start..end).contains(&at) && at % MAX_ALIGN == 0));
        }
    }

    /// A callable that records its special members.
    struct Probe {
        log: Rc<RefCell<Vec<String>>>,
        copies_left: Rc<Cell<u32>>,
        id: i64,
        moved: bool,
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            let state = if self.moved { "moved-from" } else { "live" };
            self.log
                .borrow_mut()
                .push(format!("destroy {} {state}", self.id));
        }
    }

    impl Callable<(i64,), i64> for Probe {
        fn invoke(&self, args: (i64,)) -> i64 {
            self.id + args.0
        }

        fn move_construct(&mut self) -> Option<Self> {
            self.log.borrow_mut().push(format!("move {}", self.id));
            self.moved = true;
            Some(Probe {
                log: self.log.clone(),
                copies_left: self.copies_left.clone(),
                id: self.id,
                moved: false,
            })
        }

        fn copy_construct(&self) -> Result<Self, CopyFailure> {
            if self.copies_left.get() == 0 {
                self.log
                    .borrow_mut()
                    .push(format!("copy-failed {}", self.id));
                return Err(CopyFailure);
            }
            self.copies_left.set(self.copies_left.get() - 1);
            self.log.borrow_mut().push(format!("copy {}", self.id));
            Ok(Probe {
                log: self.log.clone(),
                copies_left: self.copies_left.clone(),
                id: self.id,
                moved: false,
            })
        }
    }

    fn probe(log: &Rc<RefCell<Vec<String>>>, copies: &Rc<Cell<u32>>, id: i64) -> Probe {
        Probe {
            log: log.clone(),
            copies_left: copies.clone(),
            id,
            moved: false,
        }
    }

    fn drain(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
        log.borrow_mut().drain(..).collect()
    }

    #[test]
    fn moves_run_the_move_constructor_and_destroy_the_source() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let copies = Rc::new(Cell::new(1));
        let mut a: InplaceFunction<'_, (i64,), i64> =
            InplaceFunction::from_callable(probe(&log, &copies, 1));
        assert_eq!(drain(&log), ["move 1", "destroy 1 moved-from"]);

        let b = InplaceFunction::take(&mut a);
        assert_eq!(drain(&log), ["move 1", "destroy 1 moved-from"]);
        assert!(!a.is_set());
        assert_eq!(b.call((2,)), Ok(3));

        let mut c: InplaceFunction<'_, (i64,), i64> =
            InplaceFunction::from_callable(probe(&log, &copies, 7));
        drain(&log);
        c.assign_callable(probe(&log, &copies, 9));
        assert_eq!(
            drain(&log),
            [
                "move 9",
                "destroy 7 live",
                "move 9",
                "destroy 9 moved-from",
                "destroy 9 moved-from"
            ]
        );
        drop(b);
        assert_eq!(drain(&log), ["destroy 1 live"]);
        drop(c);
        assert_eq!(drain(&log), ["destroy 9 live"]);
    }

    #[test]
    fn failing_copy_keeps_the_old_target() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let copies = Rc::new(Cell::new(0));
        let source: InplaceFunction<'_, (i64,), i64> =
            InplaceFunction::from_callable(probe(&log, &copies, 1));
        let mut target: InplaceFunction<'_, (i64,), i64> =
            InplaceFunction::from_callable(probe(&log, &copies, 5));
        drain(&log);

        assert_eq!(target.try_copy_assign(&source), Err(CopyFailure));
        assert_eq!(drain(&log), ["copy-failed 1"]);
        assert_eq!(target.call((0,)), Ok(5));
        assert!(source.try_clone().is_err());
        drain(&log);

        copies.set(1);
        assert_eq!(target.try_copy_assign(&source), Ok(()));
        assert_eq!(
            drain(&log),
            ["copy 1", "destroy 5 live", "move 1", "destroy 1 moved-from"]
        );
        assert_eq!(target.call((0,)), Ok(1));
        assert_eq!(target.try_copy_assign_self(), Ok(()));
        target.move_assign_self();
        assert!(drain(&log).is_empty());
        assert_eq!(target.call((0,)), Ok(1));
    }
}
