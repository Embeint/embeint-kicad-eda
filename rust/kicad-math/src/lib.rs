// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Portable port of KiCad's `kimath` headers at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! [`vector`] is the canonical minimal co-port of `VECTOR2`/`VECTOR3` from
//! `libs.kimath.include.math.application.ui.p002`, which remains that unit's work; later ports
//! extend those types rather than adding others. `TYPE_SUPPORT.md` records exactly which C++
//! declarations it covers and which it does not.

pub mod vector;

pub use vector::{Vector2, Vector2D, Vector2I, Vector2L, Vector3, Vector3D, Vector3I, Vector3U};
