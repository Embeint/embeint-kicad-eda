// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

//! Port of unit `libs.kimath.include.application.pure.p001` at KiCad pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7: the header-only utilities in
//! `libs/kimath/include/{bezier_curves.h,hash.h,hash_128.h,mmh3_hash.h,md5_hash.h,transform.h}`.
//!
//! Every C++ `size_t`, `uint64_t` and `uint32_t` is modelled with a fixed-width
//! Rust integer (`u64`/`u32`), never `usize`, so the 64-bit Linux x86-64 results
//! are the same natively and on `wasm32-unknown-unknown`. Byte order follows the
//! qualified little-endian platform explicitly.
//!
//! The owned `bezier_curves.h` definitions ([`bezier`]) use the canonical
//! `kicad_math::vector::Vector2` and the minimal `VECTOR2` arithmetic co-ported there;
//! `BEZIER<double>` NaN results follow the bound reference build site by site
//! (`bezier_f64_sites`, through the private SSE2 scalar interface `sse2`).
//!
//! Unit `libs.kimath.src.transform.application.ui.p001` adds the out-of-line
//! `libs/kimath/src/transform.cpp` bodies to [`Transform`] in [`transform_source`], and unit
//! `libs.kimath.src.bezier_curves.application.ui.p001` the `libs/kimath/src/bezier_curves.cpp`
//! bodies to [`BezierPoly`] and [`Bezier`] in [`bezier_curves`].

pub mod bezier;
pub mod bezier_curves;
mod bezier_f64_sites;
pub mod hash;
pub mod hash_128;
pub mod md5_hash;
pub mod mmh3_hash;
mod sse2;
pub mod transform;
pub mod transform_source;

pub use bezier::{Bezier, BezierCoordinate, BezierPoly};
pub use hash::{hash_bytes, hash_combine, hash_val, StdHash, STD_HASH_SEED};
pub use hash_128::{CxxNumericGrouping, Hash128};
pub use md5_hash::Md5Hash;
pub use mmh3_hash::{fmix64, mmh3_rotl64, BlockReader, DomainError, Mmh3Error, Mmh3Hash};
pub use transform::Transform;
