// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/geometry_utils.h.
// Copyright (C) 2018-2019 Jean-Pierre Charras, jp.charras at wanadoo.fr.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! The inline templates and types of libs/kimath/include/geometry/geometry_utils.h at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! Instantiations (the template parameter is part of each name):
//!
//! - `GetVectorSnapped45<T>` and `GetVectorSnapped90<T>` for `int` (edit_relations.cpp,
//!   two_point_geom_manager.h, drawing_tool.cpp) and `double` (polygon_geom_manager.cpp; the
//!   `double` `GetVectorSnapped90` has no pinned caller and is ported with it).
//! - `GetClampedCoords<in_type, ret_type, unsigned int>` for `<int, int>` (drawing_tool.cpp,
//!   item_modification_routine.cpp), `<double, double>` (wx_view_controls.cpp,
//!   tool_dispatcher.cpp, the router tools) and `<double, int>` (pcb_point_editor.cpp).
//! - `IsVec2SafeXY<T>` for `double` (reference_image.cpp) and `int` (no pinned caller).
//!
//! `GetArcToSegmentCount`, `CircleToEndSegmentDeltaRadius`, `DISABLE_ARC_RADIUS_CORRECTION`,
//! `GetCircleToPolyCorrection`, `ClipLine`, `KIGEOM::BoxHitTest` and `KIGEOM::ShapeHitTest` are
//! only declared here; their bodies are geometry_utils.cpp's
//! (`libs.kimath.src.geometry.geometry_utils.application.ui.p001`).
//!
//! The undefined operations of the integral instantiations are `std::abs( INT_MIN )` and a
//! padding above `INT_MAX`, which makes `std::clamp`'s lower bound exceed its upper bound
//! ([alg.clamp]); both return [`GeometryUtilsError::Undefined`]. Floating values are only
//! compared, copied, sign-copied (`std::copysign`, a bit operation that keeps NaN payloads) or
//! rounded by the accepted `KiROUND` port, so every result bit is the reference's.
use crate::util::{clamp, ki_round};
use crate::vector::{Vector2, Vector2D, Vector2I};

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.pure.p001";

/// The pinned C++ evaluation is undefined for this input; no value exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryUtilsError {
    Undefined,
}

pub type GeometryUtilsResult<T> = Result<T, GeometryUtilsError>;

/// `LEADER_MODE`: the kind of the leader line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum LeaderMode {
    /// Unconstrained point-to-point.
    Direct = 0,
    /// 45 Degree only.
    Deg45 = 1,
    /// 90 Degree only.
    Deg90 = 2,
}

/// `std::abs( int )`, undefined for `INT_MIN`.
fn abs_i(v: i32) -> GeometryUtilsResult<i32> {
    v.checked_abs().ok_or(GeometryUtilsError::Undefined)
}

/// `std::copysign( int, int )`: both promoted to `double`, converted back on assignment to an
/// `int` coordinate. The magnitude is at most `INT_MAX` here (the caller's `std::abs` of both
/// coordinates is defined), so the result is exactly representable.
fn copysign_i(magnitude: i32, sign: i32) -> i32 {
    let m = magnitude.unsigned_abs();
    // m <= INT_MAX (INT_MIN was refused); the sign of an int zero is positive.
    let m = m as i32;
    if sign < 0 {
        -m
    } else {
        m
    }
}

/// `GetVectorSnapped45<int>( aVec, only45 = false )`: snap onto the nearest 0, 45 or 90 degree
/// line; `ext_type` is `int64_t`.
pub fn get_vector_snapped45_i(vec: Vector2I, only45: bool) -> GeometryUtilsResult<Vector2I> {
    let mut new_vec = vec;
    let abs_vec = Vector2::new(abs_i(vec.x)?, abs_i(vec.y)?);

    if !only45 && i64::from(abs_vec.x) > i64::from(abs_vec.y) * 2 {
        // snap along x-axis
        new_vec.y = 0;
    } else if !only45 && i64::from(abs_vec.y) > i64::from(abs_vec.x) * 2 {
        // snap onto y-axis
        new_vec.x = 0;
    } else if abs_vec.x > abs_vec.y {
        // snap away from x-axis towards 45
        new_vec.y = copysign_i(vec.x, vec.y);
    } else {
        // snap away from y-axis towards 45
        new_vec.x = copysign_i(vec.y, vec.x);
    }

    Ok(new_vec)
}

/// `GetVectorSnapped45<double>( aVec, only45 = false )`; `ext_type` is `double`, so the
/// doubling is `absVec * 2.0` (it may overflow to infinity, which only affects the comparison).
pub fn get_vector_snapped45_d(vec: Vector2D, only45: bool) -> Vector2D {
    let mut new_vec = vec;
    let abs_vec = Vector2::new(vec.x.abs(), vec.y.abs());

    if !only45 && abs_vec.x > abs_vec.y * 2.0 {
        new_vec.y = 0.0;
    } else if !only45 && abs_vec.y > abs_vec.x * 2.0 {
        new_vec.x = 0.0;
    } else if abs_vec.x > abs_vec.y {
        new_vec.y = vec.x.copysign(vec.y);
    } else {
        new_vec.x = vec.y.copysign(vec.x);
    }

    new_vec
}

/// `GetVectorSnapped90<int>`: snap onto the nearest horizontal or vertical line.
pub fn get_vector_snapped90_i(vec: Vector2I) -> GeometryUtilsResult<Vector2I> {
    let mut new_vec = vec;
    let abs_vec = Vector2::new(abs_i(vec.x)?, abs_i(vec.y)?);

    if abs_vec.x >= abs_vec.y {
        new_vec.y = 0;
    } else {
        new_vec.x = 0;
    }

    Ok(new_vec)
}

/// `GetVectorSnapped90<double>`.
pub fn get_vector_snapped90_d(vec: Vector2D) -> Vector2D {
    let mut new_vec = vec;
    let abs_vec = Vector2::new(vec.x.abs(), vec.y.abs());

    if abs_vec.x >= abs_vec.y {
        new_vec.y = 0.0;
    } else {
        new_vec.x = 0.0;
    }

    new_vec
}

/// The integral bounds `-( INT_MAX - aPadding ), INT_MAX - aPadding` (`int64_t` arithmetic, the
/// `unsigned int` padding converted to `int64_t`).
fn integral_bounds(padding: u32) -> GeometryUtilsResult<(i64, i64)> {
    let max = i64::from(i32::MAX) - i64::from(padding);
    let min = -max;
    if max < min {
        // std::clamp( v, lo, hi ) requires !( hi < lo ).
        return Err(GeometryUtilsError::Undefined);
    }
    Ok((min, max))
}

/// The floating bounds `-( (double) INT_MAX - aPadding ), (double) INT_MAX - aPadding`: exact.
fn floating_bounds(padding: u32) -> GeometryUtilsResult<(f64, f64)> {
    let max = f64::from(i32::MAX) - f64::from(padding);
    let min = -max;
    if max < min {
        return Err(GeometryUtilsError::Undefined);
    }
    Ok((min, max))
}

/// `GetClampedCoords<int, int>( aCoords, aPadding = 1u )`.
pub fn get_clamped_coords_i(coords: Vector2I, padding: u32) -> GeometryUtilsResult<Vector2I> {
    let (min, max) = integral_bounds(padding)?;
    let undefined = |_| GeometryUtilsError::Undefined;
    let x = clamp(i64::from(coords.x), min, max).map_err(undefined)?;
    let y = clamp(i64::from(coords.y), min, max).map_err(undefined)?;
    // |x|, |y| <= INT_MAX - padding, so the assignment to int is exact.
    Ok(Vector2::new(x as i32, y as i32))
}

/// The clamped `double` coordinates of the floating instantiations. `std::clamp` is
/// `std::min( std::max( v, lo ), hi )` (GCC 13.3 libstdc++), so a NaN is returned unchanged and
/// equal values keep the first operand.
fn clamp_d(coords: Vector2D, padding: u32) -> GeometryUtilsResult<(f64, f64)> {
    let (min, max) = floating_bounds(padding)?;
    let undefined = |_| GeometryUtilsError::Undefined;
    Ok((
        clamp(coords.x, min, max).map_err(undefined)?,
        clamp(coords.y, min, max).map_err(undefined)?,
    ))
}

/// `GetClampedCoords<double, double>( aCoords, aPadding = 1u )`.
pub fn get_clamped_coords_d(coords: Vector2D, padding: u32) -> GeometryUtilsResult<Vector2D> {
    let (x, y) = clamp_d(coords, padding)?;
    Ok(Vector2::new(x, y))
}

/// `GetClampedCoords<double, int>( aCoords, aPadding = 1u )`: the clamped values rounded with
/// `KiROUND<double, int>( v, true )` (quiet, so no overflow is logged; a NaN becomes 0).
pub fn get_clamped_coords_d_to_i(coords: Vector2D, padding: u32) -> GeometryUtilsResult<Vector2I> {
    let (x, y) = clamp_d(coords, padding)?;
    // Quiet rounding of a clamped value never logs, so it never reaches the deferred
    // kimathLogOverflow; any error would be a domain error of KiROUND itself.
    let round = |v: f64| ki_round::<f64, i32>(v, true).map_err(|_| GeometryUtilsError::Undefined);
    Ok(Vector2::new(round(x)?, round(y)?))
}

/// `IsVec2SafeXY<double>`: both coordinates strictly inside `( INT_MIN, INT_MAX )` as doubles.
pub fn is_vec2_safe_xy_d(vec: Vector2D) -> bool {
    let min = f64::from(i32::MIN);
    let max = f64::from(i32::MAX);
    vec.x > min && vec.x < max && vec.y > min && vec.y < max
}

/// `IsVec2SafeXY<int>`.
pub fn is_vec2_safe_xy_i(vec: Vector2I) -> bool {
    let min = i32::MIN;
    let max = i32::MAX;
    vec.x > min && vec.x < max && vec.y > min && vec.y < max
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vi(x: i32, y: i32) -> Vector2I {
        Vector2::new(x, y)
    }

    #[test]
    fn snapping() {
        assert_eq!(get_vector_snapped45_i(vi(10, 3), false), Ok(vi(10, 0)));
        assert_eq!(get_vector_snapped45_i(vi(3, -10), false), Ok(vi(0, -10)));
        assert_eq!(get_vector_snapped45_i(vi(10, -6), false), Ok(vi(10, -10)));
        assert_eq!(get_vector_snapped45_i(vi(-6, 10), false), Ok(vi(-10, 10)));
        assert_eq!(get_vector_snapped45_i(vi(10, 3), true), Ok(vi(10, 10)));
        assert_eq!(get_vector_snapped45_i(vi(0, 0), true), Ok(vi(0, 0)));
        assert_eq!(
            get_vector_snapped45_i(vi(i32::MIN, 0), false),
            Err(GeometryUtilsError::Undefined)
        );
        assert_eq!(get_vector_snapped90_i(vi(5, -5)), Ok(vi(5, 0)));
        let d = get_vector_snapped45_d(Vector2::new(-0.0, f64::NAN), false);
        assert_eq!(d.y.to_bits(), f64::NAN.to_bits());
        assert_eq!(d.x.to_bits(), f64::NAN.copysign(-0.0).to_bits());
    }

    #[test]
    fn clamping() {
        assert_eq!(
            get_clamped_coords_i(vi(i32::MIN, i32::MAX), 1),
            Ok(vi(-(i32::MAX - 1), i32::MAX - 1))
        );
        assert_eq!(
            get_clamped_coords_i(vi(5, -5), i32::MAX as u32),
            Ok(vi(0, 0))
        );
        assert_eq!(
            get_clamped_coords_i(vi(5, -5), i32::MAX as u32 + 1),
            Err(GeometryUtilsError::Undefined)
        );
        let z = get_clamped_coords_d(Vector2::new(-1.0, 1.0), i32::MAX as u32);
        assert_eq!(
            z.map(|v| (v.x.to_bits(), v.y.to_bits())),
            Ok(((-0.0f64).to_bits(), 0))
        );
        assert_eq!(
            get_clamped_coords_d_to_i(Vector2::new(f64::NAN, 1e300), 0),
            Ok(vi(0, i32::MAX))
        );
        assert!(!is_vec2_safe_xy_d(Vector2::new(f64::from(i32::MAX), 0.0)));
        assert!(is_vec2_safe_xy_d(Vector2::new(2147483646.5, -2147483647.5)));
    }
}
