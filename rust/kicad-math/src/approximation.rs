// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/approximation.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `ERROR_LOC` (libs/kimath/include/geometry/approximation.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`. The header has no function.

/// `ERROR_LOC`: where the error of a polygonal approximation of a curve lies, relative to the
/// curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorLoc {
    /// `ERROR_OUTSIDE = 0`.
    ErrorOutside = 0,
    /// `ERROR_INSIDE`.
    ErrorInside = 1,
}
