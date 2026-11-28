// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/corner_strategy.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `CORNER_STRATEGY` (libs/kimath/include/geometry/corner_strategy.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`. The header has no function.

/// `enum class CORNER_STRATEGY`: how an inflate transform builds the inflated polygon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum CornerStrategy {
    /// Just inflate the polygon. Acute angles create spikes.
    AllowAcuteCorners = 0,
    /// Acute angles are chamfered.
    ChamferAcuteCorners = 1,
    /// Acute angles are rounded.
    RoundAcuteCorners = 2,
    /// All angles are chamfered. The distance between new and old polygon edges is not
    /// constant, but do not change a lot.
    ChamferAllCorners = 3,
    /// All angles are rounded. The distance between new and old polygon edges is constant.
    RoundAllCorners = 4,
}
