// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kimath/include/hash.h`: `hash_combine` and `hash_val`.
//!
//! The templates call `std::hash<T>` for every argument. Their results on the
//! qualified platform (64-bit Linux, libstdc++) are reproduced by [`StdHash`]:
//! integral and `bool` values convert to `size_t`, floating values hash their
//! object bytes with `std::_Hash_bytes` (zero, including negative zero, hashes
//! to 0), and `std::string` hashes its bytes. `size_t` is always `u64` here.

/// Seed libstdc++ passes to `std::_Hash_bytes` for `std::_Hash_impl::hash`.
pub const STD_HASH_SEED: u64 = 0xc70f_6907;

/// Initial seed of `hash_val`.
const HASH_VAL_SEED: u64 = 0xa82d_e1c0;

/// The value of `std::hash<T>()( val )` for the 64-bit libstdc++ oracle.
pub trait StdHash {
    fn std_hash(&self) -> u64;
}

macro_rules! integral_hash {
    ($($type:ty),*) => {$(
        impl StdHash for $type {
            /// libstdc++ returns `static_cast<size_t>( val )`: signed values
            /// sign-extend before the conversion to the unsigned 64-bit result.
            fn std_hash(&self) -> u64 {
                *self as u64
            }
        }
    )*};
}

integral_hash!(i8, u8, i16, u16, i32, u32, i64, u64);

impl StdHash for bool {
    fn std_hash(&self) -> u64 {
        u64::from(*self)
    }
}

impl StdHash for f32 {
    /// `val != 0.0f ? _Hash_bytes( &val, 4, seed ) : 0`; NaN is nonzero.
    fn std_hash(&self) -> u64 {
        if *self != 0.0 {
            hash_bytes(&self.to_le_bytes(), STD_HASH_SEED)
        } else {
            0
        }
    }
}

impl StdHash for f64 {
    /// `val != 0.0 ? _Hash_bytes( &val, 8, seed ) : 0`; NaN is nonzero.
    fn std_hash(&self) -> u64 {
        if *self != 0.0 {
            hash_bytes(&self.to_le_bytes(), STD_HASH_SEED)
        } else {
            0
        }
    }
}

/// `std::hash<std::string>` hashes the string's bytes, which need not be UTF-8.
impl StdHash for [u8] {
    fn std_hash(&self) -> u64 {
        hash_bytes(self, STD_HASH_SEED)
    }
}

impl StdHash for str {
    fn std_hash(&self) -> u64 {
        self.as_bytes().std_hash()
    }
}

impl StdHash for String {
    fn std_hash(&self) -> u64 {
        self.as_bytes().std_hash()
    }
}

impl<T: StdHash + ?Sized> StdHash for &T {
    fn std_hash(&self) -> u64 {
        (**self).std_hash()
    }
}

fn shift_mix(value: u64) -> u64 {
    value ^ (value >> 47)
}

/// libstdc++'s 64-bit `std::_Hash_bytes( ptr, len, seed )` (`libsupc++/hash_bytes.cc`).
pub fn hash_bytes(bytes: &[u8], seed: u64) -> u64 {
    const MUL: u64 = (0xc6a4_a793_u64 << 32) + 0x5bd1_e995;
    // `len` is a size_t; a slice length always fits in 64 bits.
    let len = bytes.len() as u64;
    let mut hash = seed ^ len.wrapping_mul(MUL);
    let mut words = bytes.chunks_exact(8);
    for word in &mut words {
        let mut load = [0_u8; 8];
        load.copy_from_slice(word);
        let data = shift_mix(u64::from_le_bytes(load).wrapping_mul(MUL)).wrapping_mul(MUL);
        hash ^= data;
        hash = hash.wrapping_mul(MUL);
    }
    let tail = words.remainder();
    if !tail.is_empty() {
        // load_bytes: the last byte is the most significant.
        let data = tail
            .iter()
            .rev()
            .fold(0_u64, |result, byte| (result << 8) + u64::from(*byte));
        hash ^= data;
        hash = hash.wrapping_mul(MUL);
    }
    hash = shift_mix(hash).wrapping_mul(MUL);
    shift_mix(hash)
}

/// `hash_combine( seed, val, args... )`, including the empty base case
/// `hash_combine( seed )`, which leaves `seed` unchanged.
///
/// The C++ recursion folds the arguments from left to right, each step reading
/// the seed produced by the previous one:
/// `seed ^= std::hash<T>()( val ) + 0x9e3779b9 + ( seed << 6 ) + ( seed >> 2 )`.
/// The iteration here performs the same steps in the same order without
/// input-driven recursion.
pub fn hash_combine(seed: &mut u64, values: &[&dyn StdHash]) {
    for value in values {
        let mixed = value
            .std_hash()
            .wrapping_add(0x9e37_79b9)
            .wrapping_add(*seed << 6)
            .wrapping_add(*seed >> 2);
        *seed ^= mixed;
    }
}

/// `hash_val( args... )`: `hash_combine` from the seed `0xa82de1c0`.
pub fn hash_val(values: &[&dyn StdHash]) -> u64 {
    let mut seed = HASH_VAL_SEED;
    hash_combine(&mut seed, values);
    seed
}
