// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Portable port of KiCad's `kimath` headers at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Unit `libs.kimath.include.math.application.pure.p001` owns [`matrix3x3`]
//! (`math/matrix3x3.h`: 20 definitions) and [`wide_int`] (`math/wide_int.h`: 4 definitions).
//! [`stream`] models the caller's `std::ostream` state that `operator<<` formats with.
//! [`vector`] is the canonical minimal co-port of `VECTOR2`/`VECTOR3` from
//! `libs.kimath.include.math.application.ui.p002`, which remains that unit's work; later ports
//! extend those types rather than adding others.

pub mod matrix3x3;
pub mod stream;
pub mod vector;
pub mod wide_int;

pub use matrix3x3::{Matrix3x3, Matrix3x3D, MatrixElement, MatrixError};
pub use stream::{FmtFlags, StreamError, StreamInsert, StreamState, MAX_STREAM_FIELD};
pub use vector::{Vector2, Vector2D, Vector2I, Vector2L, Vector3, Vector3D, Vector3I, Vector3U};
pub use wide_int::{KiInt128, KiUint128, WideIntError};
