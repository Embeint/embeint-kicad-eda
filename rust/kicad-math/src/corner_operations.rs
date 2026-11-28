// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/corner_operations.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! The parameter and result types of libs/kimath/include/geometry/corner_operations.h at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! `ComputeChamferPoints` and `ComputeDogbone` are only declared here; their bodies are in
//! corner_operations.cpp (`libs.kimath.src.geometry.corner_operations.application.ui.p001`).
use crate::seg_header::Seg;
use crate::shape_arc::ShapeArc;

/// `CHAMFER_PARAMS`, an aggregate without member initializers. `Default` is its value
/// initialization (`CHAMFER_PARAMS{}`, both zero); default initialization leaves the members
/// indeterminate in C++ and has no Rust counterpart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChamferParams {
    /// Chamfer set-back distance along the first line.
    pub chamfer_setback_a: i32,
    /// Chamfer set-back distance along the second line.
    pub chamfer_setback_b: i32,
}

/// `CHAMFER_RESULT`. `Default` is `CHAMFER_RESULT{}`: `SEG()` and two empty optionals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChamferResult {
    /// The chamfer segment.
    pub chamfer: Seg,
    /// The updated original segments. These can be empty if the chamfer "consumed" the
    /// original segments.
    pub updated_seg_a: Option<Seg>,
    pub updated_seg_b: Option<Seg>,
}

/// `DOGBONE_RESULT`. `Default` is `DOGBONE_RESULT{}`: `SHAPE_ARC()`, two empty optionals and a
/// false `m_small_arc_mouth`. There is no `Clone`, because the `SHAPE_ARC` copy constructor is
/// out-of-line and not ported.
#[derive(Debug, Default)]
pub struct DogboneResult {
    pub arc: ShapeArc,
    /// The updated original segments. These can be empty if the dogbone "consumed" the
    /// original segments.
    pub updated_seg_a: Option<Seg>,
    pub updated_seg_b: Option<Seg>,
    /// If the arc mouth is smaller than the radius.
    pub small_arc_mouth: bool,
}
