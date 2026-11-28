// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/oval.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! The key point flags of libs/kimath/include/geometry/oval.h at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! `KIGEOM::ConvertToChain` and `KIGEOM::GetOvalKeyPoints` are only declared here; their bodies
//! are in oval.cpp (`libs.kimath.src.geometry.oval.application.ui.p001`).

/// `KIGEOM::OVAL_KEY_POINT_FLAGS`.
pub type OvalKeyPointFlags = u32;

/// `KIGEOM::OVAL_KEY_POINTS`: an unscoped enum used as bit flags, so its enumerators are the
/// `int` constants below.
pub type OvalKeyPoints = i32;
pub const OVAL_CENTER: OvalKeyPoints = 1 << 0;
pub const OVAL_CAP_TIPS: OvalKeyPoints = 1 << 1;
pub const OVAL_CAP_CENTERS: OvalKeyPoints = 1 << 2;
pub const OVAL_SIDE_MIDPOINTS: OvalKeyPoints = 1 << 3;
pub const OVAL_SIDE_ENDS: OvalKeyPoints = 1 << 4;
pub const OVAL_CARDINAL_EXTREMES: OvalKeyPoints = 1 << 5;
pub const OVAL_ALL_KEY_POINTS: OvalKeyPoints = 0xFF;
