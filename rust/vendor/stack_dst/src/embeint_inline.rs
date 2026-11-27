// SPDX-License-Identifier: GPL-3.0-or-later
//! Embeint patch (not part of upstream stack_dst 0.8.2): a fixed inline buffer whose
//! payload is `BYTES` bytes at a 16-byte (`std::max_align_t`) alignment, followed by a
//! separate word for the `dyn` metadata.
//!
//! `Value<dyn Trait, MaxAlignBuf<BYTES>>` stores any `U: Trait` with
//! `size_of::<U>() <= BYTES` and `align_of::<U>() <= 16` without allocating; the metadata
//! never takes payload space. The buffer is viewed as a slice of 16-byte words:
//!
//! ```text
//! offset 0                 BYTES        round_up(BYTES, 16)       + 16
//! | payload: BYTES bytes   | (padding)  | metadata word: 16 bytes |
//! ```
//!
//! `Value` writes the object at the start and metadata into the last word. Union moves
//! preserve every byte, including the tail bytes introduced by alignment rounding; struct
//! moves need not preserve padding. Initial zeroing includes unused metadata bytes copied
//! when rebuilding pointers. A value larger than `BYTES` can fit the rounded
//! storage; callers needing the exact payload limit check it themselves (the port does,
//! at compile time).

use core::mem::{self, ManuallyDrop, MaybeUninit};
use core::slice;

/// One 16-byte, 16-aligned word: the buffer's element type.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct MaxAlignWord([u8; 16]);

// SAFETY: a byte array with no padding is valid for every bit pattern.
unsafe impl crate::Pod for MaxAlignWord {
    fn default() -> Self {
        MaxAlignWord([0; 16])
    }
}

/// The inline buffer: at least `BYTES` payload bytes, then one metadata word.
/// Union moves preserve the complete representation, unlike a struct's padding.
#[repr(C, align(16))]
pub union MaxAlignBuf<const BYTES: usize> {
    // Together these fields cover every byte, including each other's padding.
    // Access is only through the DataBuf word slice.
    head: ManuallyDrop<HeadAligned<BYTES>>,
    tail: ManuallyDrop<TailAligned<BYTES>>,
}

#[repr(C)]
struct HeadAligned<const BYTES: usize> {
    alignment: MaybeUninit<MaxAlignWord>,
    bytes: [MaybeUninit<u8>; BYTES],
}

#[repr(C)]
struct TailAligned<const BYTES: usize> {
    bytes: [MaybeUninit<u8>; BYTES],
    metadata: MaybeUninit<MaxAlignWord>,
}

impl<const BYTES: usize> MaxAlignBuf<BYTES> {
    /// The number of 16-byte words the buffer exposes (payload rounded up, plus metadata).
    const WORDS: usize = mem::size_of::<Self>() / mem::size_of::<MaxAlignWord>();
}

impl<const BYTES: usize> Default for MaxAlignBuf<BYTES> {
    fn default() -> Self {
        // SAFETY: every bit pattern is valid for both union fields. Zero the whole union,
        // including rounded bytes, so metadata inspection never reads uninitialized bytes.
        unsafe { MaybeUninit::<Self>::zeroed().assume_init() }
    }
}

// SAFETY: `as_ref` and `as_mut` view the same `size_of::<Self>()` bytes of `self` (a whole
// number of `MaxAlignWord`s: the union is 16-aligned and its size a multiple of 16) as
// `MaybeUninit` words, which accept any byte, initialised or not; the pointer is stable
// until the buffer moves; `extend` never changes the data.
unsafe impl<const BYTES: usize> crate::DataBuf for MaxAlignBuf<BYTES> {
    type Inner = MaxAlignWord;

    fn as_ref(&self) -> &[MaybeUninit<MaxAlignWord>] {
        // SAFETY: see the impl comment; the slice lies inside `*self` and borrows it.
        unsafe { slice::from_raw_parts(self as *const Self as *const _, Self::WORDS) }
    }

    fn as_mut(&mut self) -> &mut [MaybeUninit<MaxAlignWord>] {
        // SAFETY: as for `as_ref`, with the unique borrow of `*self`.
        unsafe { slice::from_raw_parts_mut(self as *mut Self as *mut _, Self::WORDS) }
    }

    fn extend(&mut self, len: usize) -> Result<(), ()> {
        if len > Self::WORDS {
            Err(())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::Value;

    trait Sum {
        fn sum(&self) -> u64;
        fn address(&self) -> usize;
    }

    #[repr(align(16))]
    struct Full([u64; 4]);

    impl Sum for Full {
        fn sum(&self) -> u64 {
            self.0.iter().sum()
        }
        fn address(&self) -> usize {
            self as *const Self as usize
        }
    }

    #[test]
    fn layout_keeps_metadata_out_of_the_payload() {
        assert_eq!(mem::align_of::<MaxAlignBuf<20>>(), 16);
        assert_eq!(mem::size_of::<MaxAlignBuf<0>>(), 16);
        assert_eq!(mem::size_of::<MaxAlignBuf<16>>(), 32);
        assert_eq!(mem::size_of::<MaxAlignBuf<20>>(), 48);
        assert_eq!(mem::size_of::<MaxAlignBuf<32>>(), 48);
    }

    #[test]
    fn zero_and_odd_payload_capacities_survive_moves() {
        struct Bytes<const N: usize>([u8; N]);
        impl<const N: usize> Sum for Bytes<N> {
            fn sum(&self) -> u64 {
                self.0.iter().map(|&b| u64::from(b)).sum()
            }
            fn address(&self) -> usize {
                self as *const Self as usize
            }
        }
        fn check<const N: usize>() {
            let value =
                match Value::<dyn Sum, MaxAlignBuf<N>>::new_stable(Bytes([7; N]), |p| p as _) {
                    Ok(value) => value,
                    Err(_) => panic!("advertised payload fits"),
                };
            let moved = [value];
            assert_eq!(moved[0].sum(), 7 * N as u64);
            assert_eq!(moved[0].address() % 16, 0);
        }
        check::<0>();
        check::<1>();
        check::<15>();
        check::<16>();
        check::<17>();
        check::<31>();
        check::<32>();
        check::<33>();
    }

    #[test]
    fn stores_a_full_payload_inline_and_survives_moves() {
        let value =
            match Value::<dyn Sum, MaxAlignBuf<32>>::new_stable(Full([1, 2, 3, 4]), |p| p as _) {
                Ok(value) => value,
                Err(_) => panic!("a 32-byte payload fits"),
            };
        let moved = [value];
        assert_eq!(moved[0].sum(), 10);
        let start = &moved[0] as *const _ as usize;
        assert_eq!(moved[0].address(), start);
        assert_eq!(moved[0].address() % 16, 0);
    }

    #[test]
    fn rounded_capacity_preserves_pointer_payload_and_drops_once() {
        use self::std::cell::Cell;
        use self::std::rc::Rc;

        struct PointerPayload(Rc<Cell<u32>>, [u8; 17]);
        impl Sum for PointerPayload {
            fn sum(&self) -> u64 {
                self.1.iter().map(|&b| u64::from(b)).sum()
            }
            fn address(&self) -> usize {
                self as *const Self as usize
            }
        }
        impl Drop for PointerPayload {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let drops = Rc::new(Cell::new(0));
        // A safe Value caller can fill the rounded space even though BYTES is only 17.
        // This catches losing object bytes in alignment padding during a buffer move.
        let value = match Value::<dyn Sum, MaxAlignBuf<17>>::new_stable(
            PointerPayload(drops.clone(), [7; 17]),
            |p| p as _,
        ) {
            Ok(value) => value,
            Err(_) => panic!("rounded storage fits the payload"),
        };
        let moved = [value];
        assert_eq!(moved[0].sum(), 119);
        assert_eq!(Rc::strong_count(&drops), 2);
        drop(moved);
        assert_eq!(drops.get(), 1);
        assert_eq!(Rc::strong_count(&drops), 1);
    }

    #[test]
    fn replacement_with_panicking_destructor_does_not_destroy_twice() {
        use self::std::cell::Cell;
        use self::std::panic::{catch_unwind, AssertUnwindSafe};

        struct DropProbe<'a>(&'a Cell<u32>);
        impl Sum for DropProbe<'_> {
            fn sum(&self) -> u64 {
                0
            }
            fn address(&self) -> usize {
                self as *const Self as usize
            }
        }
        impl Drop for DropProbe<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
                if self.0.get() == 1 {
                    panic!("scripted destructor failure");
                }
            }
        }
        let drops = Cell::new(0);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let mut value =
                match Value::<dyn Sum + '_, MaxAlignBuf<32>>::new_stable(DropProbe(&drops), |p| {
                    p as _
                }) {
                    Ok(value) => value,
                    Err(_) => panic!("probe fits"),
                };
            assert!(value.replace_stable(Full([0; 4]), |p| p as _).is_ok());
        }));
        assert!(outcome.is_err());
        assert_eq!(drops.get(), 1);
    }
}
