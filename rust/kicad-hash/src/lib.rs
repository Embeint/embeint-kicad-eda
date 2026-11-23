// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Port of unit `libs.kimath.include.application.pure.p001` at KiCad pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7: the header-only utilities in
//! `libs/kimath/include/{hash.h,hash_128.h,mmh3_hash.h,md5_hash.h,transform.h}`.
//!
//! Every C++ `size_t`, `uint64_t` and `uint32_t` is modelled with a fixed-width
//! Rust integer (`u64`/`u32`), never `usize`, so the 64-bit Linux x86-64 results
//! are the same natively and on `wasm32-unknown-unknown`. Byte order follows the
//! qualified little-endian platform explicitly.
//!
//! The `BEZIER<T>` bodies and the `BEZIER_POLY( const std::vector<VECTOR2D>& )`
//! constructor in `bezier_curves.h` are owned by the same unit but are not yet
//! implemented here: they wait for the canonical `Vector2` in `kicad-math`.

pub mod hash;
pub mod hash_128;
pub mod md5_hash;
pub mod mmh3_hash;
pub mod transform;

pub use hash::{hash_bytes, hash_combine, hash_val, StdHash, STD_HASH_SEED};
pub use hash_128::{CxxNumericGrouping, GroupingTooLong, Hash128, MAX_GROUPING_BYTES};
pub use md5_hash::Md5Hash;
pub use mmh3_hash::{fmix64, mmh3_rotl64, BlockReader, DomainError, Mmh3Error, Mmh3Hash};
pub use transform::Transform;
