// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 clipper.minkowski.h (Copyright Angus Johnson 2010-2023), Boost
// Software License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.

//! `thirdparty/clipper2/Clipper2Lib/include/clipper2/clipper.minkowski.h` (Minkowski sum and
//! difference), part of unit [`crate::clipper::UNIT`], with KiCad's `USINGZ`.
//!
//! The `PathD` overloads do not check `decimalPlaces`: `pow(10, decimalPlaces)` is the pinned
//! glibc 2.39 `pow` ([`kicad_math::reference_libm::pow`]), a zero scale throws from
//! `ScalePath`, and an out-of-range coordinate conversion is refused as undefined by the core
//! port. The quadrilaterals produced are `pattern.len() * path.len()` paths, bounded by the
//! host's `max_quads` ([`ClipperError::ResourceLimit`] before anything is built).

use crate::clipper::union_subjects_64;
use crate::clipper_core::{
    is_positive, scale_path, scale_paths, FillRule, Path64, PathD, Paths64, PathsD,
};
use crate::{push, with_capacity, ClipperError, Result};
use kicad_math::reference_libm::pow;

const INDEX: ClipperError = ClipperError::Undefined("vector index out of range");

/// `tmp[i][j]`.
fn at2(tmp: &[Path64], i: usize, j: usize) -> Result<crate::clipper_core::Point64> {
    tmp.get(i).and_then(|p| p.get(j)).copied().ok_or(INDEX)
}

/// `detail::Minkowski(const Path64& pattern, const Path64& path, bool isSum, bool isClosed)`.
/// `max_quads` bounds the `pathLen * patLen` translated points and quadrilaterals.
pub fn minkowski(
    pattern: &Path64,
    path: &Path64,
    is_sum: bool,
    is_closed: bool,
    max_quads: usize,
) -> Result<Paths64> {
    let delta: usize = if is_closed { 0 } else { 1 };
    let pat_len = pattern.len();
    let path_len = path.len();
    if pat_len == 0 || path_len == 0 {
        return Ok(Paths64::new());
    }
    match pat_len.checked_mul(path_len) {
        Some(n) if n <= max_quads => {}
        _ => return Err(ClipperError::ResourceLimit),
    }
    let mut tmp: Paths64 = with_capacity(path_len)?;

    // Point64 operator+ / operator- give z = 0.
    for p in path {
        let mut path2: Path64 = with_capacity(pattern.len())?;
        for pt2 in pattern {
            path2.push(if is_sum { p.plus(pt2)? } else { p.minus(pt2)? });
        }
        tmp.push(path2);
    }

    let mut result: Paths64 = with_capacity((path_len - delta) * pat_len)?;
    let mut g = if is_closed { path_len - 1 } else { 0 };
    let mut h = pat_len - 1;
    for i in delta..path_len {
        for j in 0..pat_len {
            let mut quad: Path64 = with_capacity(4)?;
            quad.push(at2(&tmp, g, h)?);
            quad.push(at2(&tmp, i, h)?);
            quad.push(at2(&tmp, i, j)?);
            quad.push(at2(&tmp, g, j)?);
            if !is_positive(&quad)? {
                quad.reverse();
            }
            push(&mut result, quad)?;
            h = j;
        }
        g = i;
    }
    Ok(result)
}

/// `detail::Union(const Paths64& subjects, FillRule fillrule)`, the same body as `clipper.h`'s
/// `Union(const Paths64&, FillRule)`.
pub fn union(subjects: &Paths64, fillrule: FillRule) -> Result<Paths64> {
    union_subjects_64(subjects, fillrule)
}

/// `inline Paths64 MinkowskiSum(const Path64& pattern, const Path64& path, bool isClosed)`.
pub fn minkowski_sum_64(
    pattern: &Path64,
    path: &Path64,
    is_closed: bool,
    max_quads: usize,
) -> Result<Paths64> {
    union(
        &minkowski(pattern, path, true, is_closed, max_quads)?,
        FillRule::NonZero,
    )
}

/// The shared body of the two `PathD` overloads.
fn minkowski_d(
    pattern: &PathD,
    path: &PathD,
    is_sum: bool,
    is_closed: bool,
    decimal_places: i32,
    max_quads: usize,
) -> Result<PathsD> {
    let mut error_code = 0;
    let scale = pow(10.0, f64::from(decimal_places));
    let pat64 = scale_path::<i64, f64>(pattern, scale, &mut error_code)?;
    let path64 = scale_path::<i64, f64>(path, scale, &mut error_code)?;
    let tmp = union(
        &minkowski(&pat64, &path64, is_sum, is_closed, max_quads)?,
        FillRule::NonZero,
    )?;
    scale_paths::<f64, i64>(&tmp, 1.0 / scale, &mut error_code)
}

/// `inline PathsD MinkowskiSum(const PathD& pattern, const PathD& path, bool isClosed,
/// int decimalPlaces = 2)`.
pub fn minkowski_sum_d(
    pattern: &PathD,
    path: &PathD,
    is_closed: bool,
    decimal_places: i32,
    max_quads: usize,
) -> Result<PathsD> {
    minkowski_d(pattern, path, true, is_closed, decimal_places, max_quads)
}

/// `inline Paths64 MinkowskiDiff(const Path64& pattern, const Path64& path, bool isClosed)`.
pub fn minkowski_diff_64(
    pattern: &Path64,
    path: &Path64,
    is_closed: bool,
    max_quads: usize,
) -> Result<Paths64> {
    union(
        &minkowski(pattern, path, false, is_closed, max_quads)?,
        FillRule::NonZero,
    )
}

/// `inline PathsD MinkowskiDiff(const PathD& pattern, const PathD& path, bool isClosed,
/// int decimalPlaces = 2)`.
pub fn minkowski_diff_d(
    pattern: &PathD,
    path: &PathD,
    is_closed: bool,
    decimal_places: i32,
    max_quads: usize,
) -> Result<PathsD> {
    minkowski_d(pattern, path, false, is_closed, decimal_places, max_quads)
}
