// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Portable port of KiCad's `kimath` headers and math utility source at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Unit `libs.kimath.include.math.application.pure.p001` owns [`matrix3x3`]
//! (`math/matrix3x3.h`: 20 definitions) and [`wide_int`] (`math/wide_int.h`: 4 definitions).
//! [`stream`] models the caller's `std::ostream` state that `operator<<` formats with.
//! [`vector`] is the canonical minimal co-port of `VECTOR2`/`VECTOR3` from
//! `libs.kimath.include.math.application.ui.p002`, which remains that unit's work; later ports
//! extend those types rather than adding others.
//!
//! The private `glibc_trig` module reproduces the reference C library's `sin`, `cos` and
//! `sincos` (glibc 2.39, baseline SSE2 variants) bit for bit without the host libm, so
//! `SetRotation` gives the same bits natively and in WebAssembly.
//!
//! The `MATRIX3x3<double>` arithmetic is evaluated through private per-site code generated
//! from the pinned oracle binary (`generated`) on explicit SSE2 NaN selection (`sse2`); see
//! `port/findings/kimath-matrix-mixed-nan-payloads.md`.

// Lets the generated per-site code name this crate's public types as `kicad_math::...`
// exactly as the generator emits it, so the generated file stays byte-identical.
extern crate self as kicad_math;

mod fp;
pub mod math_util;

// `generated.rs` is emitter output (sha256 d24ff0f2..., from the reviewed graph locks of the
// pinned oracle; port/tools/kimath_nan/README.md). It is bound byte for byte by
// port/tools/kimath_nan/check_generated.py and the emitter's handoff check, so it is never
// edited or reformatted here. The emitter ends the file with one blank line, which rustfmt
// would remove; fixing that belongs in the emitter and a reviewed re-emission, not here.
#[rustfmt::skip]
mod generated;
mod glibc_trig;
pub mod matrix3x3;
mod sse2;
pub mod stream;
pub mod vector;
pub mod wide_int;

pub use matrix3x3::{Matrix3x3, Matrix3x3D, MatrixElement, MatrixError};
pub use stream::{FmtFlags, StreamError, StreamInsert, StreamState, MAX_STREAM_FIELD};
pub use vector::{
    Vector2, Vector2D, Vector2I, Vector2L, Vector3, Vector3D, Vector3I, Vector3U, VectorDomainError,
};
pub use wide_int::{KiInt128, KiUint128, WideIntError};
