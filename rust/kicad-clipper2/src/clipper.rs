// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 clipper.h (Copyright Angus Johnson 2010-2025), Boost Software
// License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.

//! `thirdparty/clipper2/Clipper2Lib/include/clipper2/clipper.h` ("a simple interface to the
//! Clipper Library") at KiCad pin 4cf939778df04d2b747bb2c4714fc5014a7565c7, part of unit
//! [`UNIT`], with KiCad's configuration: `USINGZ` and C++ exceptions (`DoError` throws, so the
//! "non-fatal" `error_code` continuations after `CheckPrecisionRange`, `ScalePaths` and
//! `MakePath` are compiled but unreachable).
//!
//! Every inline body of the header is ported: the boolean wrappers (`BooleanOp`, `Intersect`,
//! `Union`, `Difference`, `Xor`), `InflatePaths`, `TranslatePath(s)`, `RectClip` and
//! `RectClipLines` (including the `RectD` scaling overloads), the `details` helpers, the
//! `PolyTree` `operator<<` insertions, `PolyTreeToPaths64/D`,
//! `CheckPolytreeFullyContainsChildren`, `MakePath`, `MakePathD`, `MakePathZ`, `MakePathZD`,
//! `TrimCollinear`, `Distance`, `Length`, `NearCollinear`, `Ellipse`, `SimplifyPath(s)`,
//! `Path2ContainsPath1`, `RDP` and `RamerDouglasPeucker`.
//!
//! The templates are generic over [`Coord`] (`int64_t` and `double`, the instantiations
//! KiCad's types allow). Floating point follows the oracle's `-ffp-contract=off` in source
//! order; `std::sin`, `std::cos` and `std::pow` are the pinned glibc 2.39 bodies from
//! [`kicad_math::reference_libm`] and `std::sqrt` is exact. Undefined C++ evaluation (signed
//! overflow, out-of-range `double` to integer conversion, out-of-range indexing, a
//! non-terminating recursion) is refused with [`ClipperError::Undefined`].
//!
//! Recursions (`PolyPathToPaths64`, `PolyPath64ContainsChildren`, `OutlinePolyPath64`, `RDP`)
//! run on explicit stacks in the reference's order, so input-controlled depth never reaches
//! the native stack. Wrappers that construct a `ClipperOffset` or `RectClip64` take the host
//! budget those co-ported classes require ([`Budget`], [`RectClipBudget`]); results within the
//! budget are unchanged.

use crate::clipper_core::{
    cast_f64_to_i64, check_precision_range, cross_product3, distance_sqr, do_error,
    get_bounds_path, is_collinear, perpendic_dist_from_line_sqrd, point_in_polygon, scale_path,
    scale_paths, scale_rect, sqr, Coord, FillRule, Path, Path64, PathD, Paths, Paths64, PathsD,
    Point, PointInPolygonResult, Rect, Rect64, RectD, MAX_DBL, NON_PAIR_ERROR_I, PI,
};
use crate::clipper_engine::{
    ClipType, Clipper64, ClipperD, PolyPathId, PolyTree, PolyTree64, PolyTreeD,
};
use crate::offset::{ClipperOffset, EndType, JoinType};
use crate::rectclip::{RectClip64, RectClipBudget, RectClipLines64};
use crate::stream::{endl, insert_text, insert_u64};
use crate::{copy_of, push, reserve, with_capacity, Budget, ClipperError, Result};
use kicad_math::reference_libm::{cos, pow, sin};
use kicad_math::StreamState;

/// This port unit (`clipper.h`, `clipper.minkowski.h`, `clipper.offset.h`,
/// `clipper.rectclip.h`, `clipper.triangulation.h`, `clipper.version.h`).
pub const UNIT: &str = "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002";

const INDEX: ClipperError = ClipperError::Undefined("vector index out of range");

/// `v[index]`, refusing an index the C++ would read out of range.
fn at<T>(items: &[T], index: usize) -> Result<&T> {
    items.get(index).ok_or(INDEX)
}

/// `v[index] = value`, refusing an index the C++ would write out of range.
fn set<T>(items: &mut [T], index: usize, value: T) -> Result<()> {
    *items.get_mut(index).ok_or(INDEX)? = value;
    Ok(())
}

/// A fallible deep copy of `paths`.
fn copy_paths<T: Clone>(paths: &[Vec<T>]) -> Result<Vec<Vec<T>>> {
    let mut copy = with_capacity(paths.len())?;
    for path in paths {
        copy.push(copy_of(path)?);
    }
    Ok(copy)
}

/// `std::pow(10, precision)`: both `int` arguments promote to `double`.
fn pow10(precision: i32) -> f64 {
    pow(10.0, f64::from(precision))
}

// --- boolean operations --------------------------------------------------------------------

/// `inline Paths64 BooleanOp(ClipType cliptype, FillRule fillrule, const Paths64& subjects,
/// const Paths64& clips)`. The `Execute` success flag is discarded, as in the C++.
pub fn boolean_op_64(
    cliptype: ClipType,
    fillrule: FillRule,
    subjects: &Paths64,
    clips: &Paths64,
) -> Result<Paths64> {
    let mut result = Paths64::new();
    let mut clipper = Clipper64::default();
    clipper.add_subject(subjects)?;
    clipper.add_clip(clips)?;
    clipper.execute(cliptype, fillrule, &mut result)?;
    Ok(result)
}

/// `inline void BooleanOp(ClipType, FillRule, const Paths64& subjects, const Paths64& clips,
/// PolyTree64& solution)`: `Execute(cliptype, fillrule, solution, sol_open)` with a discarded
/// local `sol_open`, so the tree is replaced only when the execution succeeds.
pub fn boolean_op_64_tree(
    cliptype: ClipType,
    fillrule: FillRule,
    subjects: &Paths64,
    clips: &Paths64,
    solution: &mut PolyTree64,
) -> Result<()> {
    let mut sol_open = Paths64::new();
    let mut clipper = Clipper64::default();
    clipper.add_subject(subjects)?;
    clipper.add_clip(clips)?;
    clipper.execute_tree_with_open(cliptype, fillrule, solution, &mut sol_open)?;
    Ok(())
}

/// `inline PathsD BooleanOp(ClipType, FillRule, const PathsD& subjects, const PathsD& clips,
/// int precision = 2)`. An out-of-range precision throws from `CheckPrecisionRange`.
pub fn boolean_op_d(
    cliptype: ClipType,
    fillrule: FillRule,
    subjects: &PathsD,
    clips: &PathsD,
    precision: i32,
) -> Result<PathsD> {
    let mut error_code = 0;
    let mut precision = precision;
    check_precision_range(&mut precision, &mut error_code)?;
    let mut result = PathsD::new();
    if error_code != 0 {
        return Ok(result);
    }
    let mut clipper = ClipperD::new(precision)?;
    clipper.add_subject(subjects)?;
    clipper.add_clip(clips)?;
    clipper.execute(cliptype, fillrule, &mut result)?;
    Ok(result)
}

/// `inline void BooleanOp(ClipType, FillRule, const PathsD& subjects, const PathsD& clips,
/// PolyTreeD& polytree, int precision = 2)` on the tree root. The tree is cleared first, so it
/// is empty even when `CheckPrecisionRange` throws.
pub fn boolean_op_d_tree(
    cliptype: ClipType,
    fillrule: FillRule,
    subjects: &PathsD,
    clips: &PathsD,
    polytree: &mut PolyTreeD,
    precision: i32,
) -> Result<()> {
    polytree.clear(PolyTreeD::ROOT)?;
    let mut error_code = 0;
    let mut precision = precision;
    check_precision_range(&mut precision, &mut error_code)?;
    if error_code != 0 {
        return Ok(());
    }
    let mut clipper = ClipperD::new(precision)?;
    clipper.add_subject(subjects)?;
    clipper.add_clip(clips)?;
    clipper.execute_tree(cliptype, fillrule, polytree)?;
    Ok(())
}

/// `inline Paths64 Intersect(const Paths64& subjects, const Paths64& clips, FillRule)`.
pub fn intersect_64(subjects: &Paths64, clips: &Paths64, fillrule: FillRule) -> Result<Paths64> {
    boolean_op_64(ClipType::Intersection, fillrule, subjects, clips)
}

/// `inline PathsD Intersect(const PathsD&, const PathsD&, FillRule, int decimal_prec = 2)`.
pub fn intersect_d(
    subjects: &PathsD,
    clips: &PathsD,
    fillrule: FillRule,
    decimal_prec: i32,
) -> Result<PathsD> {
    boolean_op_d(
        ClipType::Intersection,
        fillrule,
        subjects,
        clips,
        decimal_prec,
    )
}

/// `inline Paths64 Union(const Paths64& subjects, const Paths64& clips, FillRule)`.
pub fn union_64(subjects: &Paths64, clips: &Paths64, fillrule: FillRule) -> Result<Paths64> {
    boolean_op_64(ClipType::Union, fillrule, subjects, clips)
}

/// `inline PathsD Union(const PathsD&, const PathsD&, FillRule, int decimal_prec = 2)`.
pub fn union_d(
    subjects: &PathsD,
    clips: &PathsD,
    fillrule: FillRule,
    decimal_prec: i32,
) -> Result<PathsD> {
    boolean_op_d(ClipType::Union, fillrule, subjects, clips, decimal_prec)
}

/// `inline Paths64 Union(const Paths64& subjects, FillRule fillrule)`.
pub fn union_subjects_64(subjects: &Paths64, fillrule: FillRule) -> Result<Paths64> {
    let mut result = Paths64::new();
    let mut clipper = Clipper64::default();
    clipper.add_subject(subjects)?;
    clipper.execute(ClipType::Union, fillrule, &mut result)?;
    Ok(result)
}

/// `inline PathsD Union(const PathsD& subjects, FillRule fillrule, int precision = 2)`.
pub fn union_subjects_d(subjects: &PathsD, fillrule: FillRule, precision: i32) -> Result<PathsD> {
    let mut result = PathsD::new();
    let mut error_code = 0;
    let mut precision = precision;
    check_precision_range(&mut precision, &mut error_code)?;
    if error_code != 0 {
        return Ok(result);
    }
    let mut clipper = ClipperD::new(precision)?;
    clipper.add_subject(subjects)?;
    clipper.execute(ClipType::Union, fillrule, &mut result)?;
    Ok(result)
}

/// `inline Paths64 Difference(const Paths64& subjects, const Paths64& clips, FillRule)`.
pub fn difference_64(subjects: &Paths64, clips: &Paths64, fillrule: FillRule) -> Result<Paths64> {
    boolean_op_64(ClipType::Difference, fillrule, subjects, clips)
}

/// `inline PathsD Difference(const PathsD&, const PathsD&, FillRule, int decimal_prec = 2)`.
pub fn difference_d(
    subjects: &PathsD,
    clips: &PathsD,
    fillrule: FillRule,
    decimal_prec: i32,
) -> Result<PathsD> {
    boolean_op_d(
        ClipType::Difference,
        fillrule,
        subjects,
        clips,
        decimal_prec,
    )
}

/// `inline Paths64 Xor(const Paths64& subjects, const Paths64& clips, FillRule)`.
pub fn xor_64(subjects: &Paths64, clips: &Paths64, fillrule: FillRule) -> Result<Paths64> {
    boolean_op_64(ClipType::Xor, fillrule, subjects, clips)
}

/// `inline PathsD Xor(const PathsD&, const PathsD&, FillRule, int decimal_prec = 2)`.
pub fn xor_d(
    subjects: &PathsD,
    clips: &PathsD,
    fillrule: FillRule,
    decimal_prec: i32,
) -> Result<PathsD> {
    boolean_op_d(ClipType::Xor, fillrule, subjects, clips, decimal_prec)
}

// --- offsetting ------------------------------------------------------------------------------

/// `inline Paths64 InflatePaths(const Paths64& paths, double delta, JoinType jt, EndType et,
/// double miter_limit = 2.0, double arc_tolerance = 0.0)`, within the host's offset `budget`.
/// A zero delta returns a copy of `paths`.
pub fn inflate_paths_64(
    paths: &Paths64,
    delta: f64,
    jt: JoinType,
    et: EndType,
    miter_limit: f64,
    arc_tolerance: f64,
    budget: Budget,
) -> Result<Paths64> {
    if delta == 0.0 {
        return copy_paths(paths);
    }
    let mut clip_offset = ClipperOffset::new(miter_limit, arc_tolerance, false, false, budget);
    clip_offset.add_paths(paths, jt, et)?;
    let mut solution = Paths64::new();
    clip_offset.execute(delta, &mut solution)?;
    Ok(solution)
}

/// `inline PathsD InflatePaths(const PathsD& paths, double delta, JoinType jt, EndType et,
/// double miter_limit = 2.0, int precision = 2, double arc_tolerance = 0.0)`, within the
/// host's offset `budget`. `CheckPrecisionRange` throws before the zero-delta shortcut.
#[allow(clippy::too_many_arguments)]
pub fn inflate_paths_d(
    paths: &PathsD,
    delta: f64,
    jt: JoinType,
    et: EndType,
    miter_limit: f64,
    precision: i32,
    arc_tolerance: f64,
    budget: Budget,
) -> Result<PathsD> {
    let mut error_code = 0;
    let mut precision = precision;
    check_precision_range(&mut precision, &mut error_code)?;
    if delta == 0.0 {
        return copy_paths(paths);
    }
    if error_code != 0 {
        return Ok(PathsD::new());
    }
    let scale = pow10(precision);
    let mut clip_offset =
        ClipperOffset::new(miter_limit, arc_tolerance * scale, false, false, budget);
    let scaled = scale_paths::<i64, f64>(paths, scale, &mut error_code)?;
    clip_offset.add_paths(&scaled, jt, et)?;
    if error_code != 0 {
        return Ok(PathsD::new());
    }
    let mut solution = Paths64::new();
    clip_offset.execute(delta * scale, &mut solution)?;
    scale_paths::<f64, i64>(&solution, 1.0 / scale, &mut error_code)
}

// --- translation ---------------------------------------------------------------------------

/// `template <typename T> inline Path<T> TranslatePath(const Path<T>& path, T dx, T dy)`; also
/// the `Path64`/`PathD` overloads. Each point is `Point<T>(pt.x + dx, pt.y + dy)`, so its `z`
/// becomes 0.
pub fn translate_path<T: Coord>(path: &[Point<T>], dx: T, dy: T) -> Result<Path<T>> {
    let mut result = with_capacity(path.len())?;
    for pt in path {
        result.push(Point::xy(pt.x.add(dx)?, pt.y.add(dy)?));
    }
    Ok(result)
}

/// `template <typename T> inline Paths<T> TranslatePaths(const Paths<T>& paths, T dx, T dy)`;
/// also the `Paths64`/`PathsD` overloads.
pub fn translate_paths<T: Coord>(paths: &[Path<T>], dx: T, dy: T) -> Result<Paths<T>> {
    let mut result = with_capacity(paths.len())?;
    for path in paths {
        result.push(translate_path(path, dx, dy)?);
    }
    Ok(result)
}

// --- rectangle clipping --------------------------------------------------------------------

/// `inline Paths64 RectClip(const Rect64& rect, const Paths64& paths)`.
pub fn rect_clip_64(rect: &Rect64, paths: &Paths64, budget: RectClipBudget) -> Result<Paths64> {
    if rect.is_empty() || paths.is_empty() {
        return Ok(Paths64::new());
    }
    let mut rc = RectClip64::new(rect, budget)?;
    rc.execute(paths)
}

/// `inline Paths64 RectClip(const Rect64& rect, const Path64& path)`.
pub fn rect_clip_64_path(rect: &Rect64, path: &Path64, budget: RectClipBudget) -> Result<Paths64> {
    if rect.is_empty() || path.is_empty() {
        return Ok(Paths64::new());
    }
    let mut rc = RectClip64::new(rect, budget)?;
    let mut paths = Paths64::new();
    push(&mut paths, copy_of(path)?)?;
    rc.execute(&paths)
}

/// `inline PathsD RectClip(const RectD& rect, const PathsD& paths, int precision = 2)`. The
/// rectangle is scaled by `ScaleRect` (no range check) and the clipper constructed before the
/// paths are scaled.
pub fn rect_clip_d(
    rect: &RectD,
    paths: &PathsD,
    precision: i32,
    budget: RectClipBudget,
) -> Result<PathsD> {
    if rect.is_empty() || paths.is_empty() {
        return Ok(PathsD::new());
    }
    let mut error_code = 0;
    let mut precision = precision;
    check_precision_range(&mut precision, &mut error_code)?;
    if error_code != 0 {
        return Ok(PathsD::new());
    }
    let scale = pow10(precision);
    let r = scale_rect::<i64, f64>(rect, scale)?;
    let mut rc = RectClip64::new(&r, budget)?;
    let pp = scale_paths::<i64, f64>(paths, scale, &mut error_code)?;
    if error_code != 0 {
        return Ok(PathsD::new()); // ie: error_code result is lost
    }
    let clipped = rc.execute(&pp)?;
    scale_paths::<f64, i64>(&clipped, 1.0 / scale, &mut error_code)
}

/// `inline PathsD RectClip(const RectD& rect, const PathD& path, int precision = 2)`:
/// `RectClip(rect, PathsD{ path }, precision)` (one path, even when it is empty).
pub fn rect_clip_d_path(
    rect: &RectD,
    path: &PathD,
    precision: i32,
    budget: RectClipBudget,
) -> Result<PathsD> {
    let mut paths = PathsD::new();
    push(&mut paths, copy_of(path)?)?;
    rect_clip_d(rect, &paths, precision, budget)
}

/// `inline Paths64 RectClipLines(const Rect64& rect, const Paths64& lines)`.
pub fn rect_clip_lines_64(
    rect: &Rect64,
    lines: &Paths64,
    budget: RectClipBudget,
) -> Result<Paths64> {
    if rect.is_empty() || lines.is_empty() {
        return Ok(Paths64::new());
    }
    let mut rcl = RectClipLines64::new(rect, budget)?;
    rcl.execute(lines)
}

/// `inline Paths64 RectClipLines(const Rect64& rect, const Path64& line)`:
/// `RectClipLines(rect, Paths64{ line })` (one path, even when it is empty).
pub fn rect_clip_lines_64_path(
    rect: &Rect64,
    line: &Path64,
    budget: RectClipBudget,
) -> Result<Paths64> {
    let mut lines = Paths64::new();
    push(&mut lines, copy_of(line)?)?;
    rect_clip_lines_64(rect, &lines, budget)
}

/// `inline PathsD RectClipLines(const RectD& rect, const PathsD& lines, int precision = 2)`.
pub fn rect_clip_lines_d(
    rect: &RectD,
    lines: &PathsD,
    precision: i32,
    budget: RectClipBudget,
) -> Result<PathsD> {
    if rect.is_empty() || lines.is_empty() {
        return Ok(PathsD::new());
    }
    let mut error_code = 0;
    let mut precision = precision;
    check_precision_range(&mut precision, &mut error_code)?;
    if error_code != 0 {
        return Ok(PathsD::new());
    }
    let scale = pow10(precision);
    let r = scale_rect::<i64, f64>(rect, scale)?;
    let mut rcl = RectClipLines64::new(&r, budget)?;
    let mut p = scale_paths::<i64, f64>(lines, scale, &mut error_code)?;
    if error_code != 0 {
        return Ok(PathsD::new());
    }
    p = rcl.execute(&p)?;
    scale_paths::<f64, i64>(&p, 1.0 / scale, &mut error_code)
}

/// `inline PathsD RectClipLines(const RectD& rect, const PathD& line, int precision = 2)`.
pub fn rect_clip_lines_d_path(
    rect: &RectD,
    line: &PathD,
    precision: i32,
    budget: RectClipBudget,
) -> Result<PathsD> {
    let mut lines = PathsD::new();
    push(&mut lines, copy_of(line)?)?;
    rect_clip_lines_d(rect, &lines, precision, budget)
}

// --- details -------------------------------------------------------------------------------

/// `details::PolyPathToPaths64(const PolyPath64& polypath, Paths64& paths)` and
/// `details::PolyPathToPathsD`: the node's polygon, then each child's subtree in order
/// (preorder), on an explicit stack.
pub fn poly_path_to_paths<T: Coord>(
    tree: &PolyTree<T>,
    polypath: PolyPathId,
    paths: &mut Paths<T>,
) -> Result<()> {
    let mut stack: Vec<PolyPathId> = Vec::new();
    push(&mut stack, polypath)?;
    while let Some(node) = stack.pop() {
        push(paths, copy_of(tree.polygon(node)?)?)?;
        let children = tree.children(node)?;
        reserve(&mut stack, children.len())?;
        // Reverse order on the stack, so the first child is visited next.
        stack.extend(children.iter().rev().copied());
    }
    Ok(())
}

/// `details::PolyPath64ContainsChildren(const PolyPath64& pp)` on an explicit stack: each
/// child's vertices are checked against its parent's polygon, then the child's own children
/// (if any) before the next sibling, returning `false` at the first child that is not fully
/// contained.
pub fn poly_path64_contains_children(tree: &PolyTree64, pp: PolyPathId) -> Result<bool> {
    // (parent, index of its next child to check)
    let mut stack: Vec<(PolyPathId, usize)> = Vec::new();
    push(&mut stack, (pp, 0))?;
    while let Some(top) = stack.last_mut() {
        let (parent, next) = *top;
        let Some(&child) = tree.children(parent)?.get(next) else {
            // every child of `parent` was contained: `return true` to its caller
            stack.pop();
            continue;
        };
        top.1 = next + 1;
        // return false if this child isn't fully contained by its parent

        // checking for a single vertex outside is a bit too crude since
        // it doesn't account for rounding errors. It's better to check
        // for consecutive vertices found outside the parent's polygon.
        let parent_polygon = tree.polygon(parent)?;
        let mut outside_cnt: i32 = 0;
        for pt in tree.polygon(child)? {
            let result = point_in_polygon(pt, parent_polygon)?;
            if result == PointInPolygonResult::IsInside {
                outside_cnt -= 1;
            } else if result == PointInPolygonResult::IsOutside {
                outside_cnt += 1;
            }
            if outside_cnt > 1 {
                return Ok(false);
            } else if outside_cnt < -1 {
                break;
            }
        }

        // now check any nested children too
        if tree.count(child)? > 0 {
            push(&mut stack, (child, 0))?;
        }
    }
    Ok(true)
}

/// `static void details::OutlinePolyPath(std::ostream& os, size_t idx, bool isHole,
/// size_t count, const std::string& preamble)` (`size_t` is 64 bits on the reference).
pub fn outline_poly_path(
    idx: u64,
    is_hole: bool,
    count: u64,
    preamble: &str,
    state: &mut StreamState,
) -> Result<String> {
    let plural = if count == 1 { "." } else { "s." };
    let (kind, noun) = if is_hole {
        ("+- Hole (", " nested polygon")
    } else {
        ("+- Polygon (", " hole")
    };
    let mut text = String::new();
    append(&mut text, &insert_text(preamble, state)?)?;
    append(&mut text, &insert_text(kind, state)?)?;
    append(&mut text, &insert_u64(idx, state)?)?;
    append(&mut text, &insert_text(") contains ", state)?)?;
    append(&mut text, &insert_u64(count, state)?)?;
    append(&mut text, &insert_text(noun, state)?)?;
    append(&mut text, &insert_text(plural, state)?)?;
    append(&mut text, endl())?;
    Ok(text)
}

/// `static void details::OutlinePolyPath64(std::ostream&, const PolyPath64& pp, size_t idx,
/// std::string preamble)` and `details::OutlinePolyPathD`: the node's outline, then each child
/// that has children of its own, recursively with `preamble + "  "`, on an explicit stack.
pub fn outline_poly_path_tree<T: Coord>(
    tree: &PolyTree<T>,
    pp: PolyPathId,
    idx: usize,
    preamble: &str,
    state: &mut StreamState,
) -> Result<String> {
    let mut text = String::new();
    // (node, depth below `pp`, index of its next child)
    let mut stack: Vec<(PolyPathId, usize, usize)> = Vec::new();
    append(
        &mut text,
        &outline_poly_path(
            idx as u64,
            tree.is_hole(pp)?,
            tree.count(pp)? as u64,
            preamble,
            state,
        )?,
    )?;
    push(&mut stack, (pp, 0, 0))?;
    while let Some(top) = stack.last_mut() {
        let (node, depth, i) = *top;
        let Some(&child) = tree.children(node)?.get(i) else {
            stack.pop();
            continue;
        };
        top.2 = i + 1;
        if tree.count(child)? != 0 {
            let child_depth = depth.checked_add(1).ok_or(ClipperError::Allocation)?;
            let child_preamble = indented(preamble, child_depth)?;
            append(
                &mut text,
                &outline_poly_path(
                    i as u64,
                    tree.is_hole(child)?,
                    tree.count(child)? as u64,
                    &child_preamble,
                    state,
                )?,
            )?;
            push(&mut stack, (child, child_depth, 0))?;
        }
    }
    Ok(text)
}

/// `preamble` followed by `depth` copies of `"  "`.
fn indented(preamble: &str, depth: usize) -> Result<String> {
    let extra = depth.checked_mul(2).ok_or(ClipperError::Allocation)?;
    let mut text = String::new();
    text.try_reserve(preamble.len().saturating_add(extra))
        .map_err(|_| ClipperError::Allocation)?;
    text.push_str(preamble);
    text.extend(::core::iter::repeat_n(' ', extra));
    Ok(text)
}

/// Append stream text, which grows with the caller's tree, fallibly.
fn append(text: &mut String, piece: &str) -> Result<()> {
    text.try_reserve(piece.len())
        .map_err(|_| ClipperError::Allocation)?;
    text.push_str(piece);
    Ok(())
}

/// The shared body of the two `operator<<(std::ostream&, const PolyTree64/D&)` insertions,
/// without the `PolyTreeD` trailer.
fn insert_polytree_body<T: Coord>(
    tree: &PolyTree<T>,
    pp: PolyPathId,
    state: &mut StreamState,
) -> Result<String> {
    let count = tree.count(pp)?;
    let plural = if count == 1 {
        " polygon."
    } else {
        " polygons."
    };
    let mut text = String::new();
    append(&mut text, endl())?;
    append(&mut text, &insert_text("Polytree with ", state)?)?;
    append(&mut text, &insert_u64(count as u64, state)?)?;
    append(&mut text, &insert_text(plural, state)?)?;
    append(&mut text, endl())?;
    for i in 0..count {
        let child = tree.child(pp, i)?;
        if tree.count(child)? != 0 {
            append(
                &mut text,
                &outline_poly_path_tree(tree, child, i, "  ", state)?,
            )?;
        }
    }
    append(&mut text, endl())?;
    append(&mut text, endl())?;
    Ok(text)
}

/// `inline std::ostream& operator<< (std::ostream& os, const PolyTree64& pp)`.
pub fn insert_polytree64(
    tree: &PolyTree64,
    pp: PolyPathId,
    state: &mut StreamState,
) -> Result<String> {
    insert_polytree_body(tree, pp, state)
}

/// `inline std::ostream& operator<< (std::ostream& os, const PolyTreeD& pp)`: as the
/// `PolyTree64` insertion, plus one more `std::endl` when `pp` is a root (`Level() == 0`).
pub fn insert_polytree_d(
    tree: &PolyTreeD,
    pp: PolyPathId,
    state: &mut StreamState,
) -> Result<String> {
    let mut text = insert_polytree_body(tree, pp, state)?;
    if tree.level(pp)? == 0 {
        append(&mut text, endl())?;
    }
    Ok(text)
}

/// `inline Paths64 PolyTreeToPaths64(const PolyTree64& polytree)` and
/// `inline PathsD PolyTreeToPathsD(const PolyTreeD& polytree)`: every node below `polytree`
/// in preorder (the node itself is not included).
pub fn poly_tree_to_paths<T: Coord>(tree: &PolyTree<T>, polytree: PolyPathId) -> Result<Paths<T>> {
    let mut result = Paths::new();
    for &child in tree.children(polytree)? {
        poly_path_to_paths(tree, child, &mut result)?;
    }
    Ok(result)
}

/// `inline bool CheckPolytreeFullyContainsChildren(const PolyTree64& polytree)`.
pub fn check_polytree_fully_contains_children(
    tree: &PolyTree64,
    polytree: PolyPathId,
) -> Result<bool> {
    for &child in tree.children(polytree)? {
        if tree.count(child)? > 0 && !poly_path64_contains_children(tree, child)? {
            return Ok(false);
        }
    }
    Ok(true)
}

// --- MakePath ------------------------------------------------------------------------------

/// The arithmetic element types `MakePathD` and `MakePathZD` accept (`char` is excluded by
/// their `enable_if`), with the C++ conversions the `Point` constructors apply to them.
pub trait PathNumber: Copy + sealed::Sealed {
    /// `std::numeric_limits<T>::is_integer`.
    const IS_INTEGER: bool;
    /// `static_cast<int64_t>(value)` for an integral type: exact for signed types, modulo
    /// 2^64 for unsigned ones (defined since C++20). `None` for a floating type.
    fn as_i64(self) -> Option<i64>;
    /// `static_cast<double>(value)`.
    fn as_f64(self) -> f64;
}

/// The integral element types `MakePath` and `MakePathZ` accept.
pub trait PathInteger: PathNumber {}

mod sealed {
    pub trait Sealed {}
}

macro_rules! integer_path_number {
    ($($t:ty),*) => {$(
        impl sealed::Sealed for $t {}
        impl PathNumber for $t {
            const IS_INTEGER: bool = true;
            fn as_i64(self) -> Option<i64> {
                Some(self as i64)
            }
            fn as_f64(self) -> f64 {
                self as f64
            }
        }
        impl PathInteger for $t {}
    )*};
}
integer_path_number!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl sealed::Sealed for f32 {}
impl PathNumber for f32 {
    const IS_INTEGER: bool = false;
    fn as_i64(self) -> Option<i64> {
        None
    }
    fn as_f64(self) -> f64 {
        f64::from(self)
    }
}

impl sealed::Sealed for f64 {}
impl PathNumber for f64 {
    const IS_INTEGER: bool = false;
    fn as_i64(self) -> Option<i64> {
        None
    }
    fn as_f64(self) -> f64 {
        self
    }
}

/// `template <typename T2> Point<T>(const T2 x_, const T2 y_, const z_type z_)`: `Init`
/// converts an integral `T2` into an integral `T` with `static_cast<int64_t>`, rounds a
/// floating `T2` into an integral `T`, and converts any `T2` into `double` with
/// `static_cast<double>`.
fn point_from<T: Coord, N: PathNumber>(x: N, y: N, z: i64) -> Result<Point<T>> {
    match (T::IS_INTEGRAL, x.as_i64(), y.as_i64()) {
        (true, Some(x), Some(y)) => Point::<T>::new(x, y, z),
        _ => Point::<T>::new(x.as_f64(), y.as_f64(), z),
    }
}

/// `details::MakePathGeneric(const T an_array, size_t array_size, std::vector<U>& result)`
/// (`USINGZ`: `emplace_back(an_array[i], an_array[i + 1], 0)`).
pub fn make_path_generic<T: Coord, N: PathNumber>(
    an_array: &[N],
    array_size: usize,
    result: &mut Path<T>,
) -> Result<()> {
    reserve(result, (array_size / 2).saturating_sub(result.len()))?;
    let mut i = 0;
    while i < array_size {
        let x = *at(an_array, i)?;
        let y = *at(an_array, i + 1)?;
        push(result, point_from::<T, N>(x, y, 0)?)?;
        i += 2;
    }
    Ok(())
}

/// `template<typename T, ...integral, not char> inline Path64 MakePath(const std::vector<T>&
/// list)`. An odd count throws (`DoError(non_pair_error_i)`).
pub fn make_path<N: PathInteger>(list: &[N]) -> Result<Path64> {
    let size = list.len() - list.len() % 2;
    if list.len() != size {
        return Err(do_error(NON_PAIR_ERROR_I)); // non-fatal without exception handling
    }
    let mut result = Path64::new();
    make_path_generic(list, size, &mut result)?;
    Ok(result)
}

/// `template<typename T, std::size_t N, ...integral, not char> inline Path64 MakePath(const
/// T(&list)[N])`: "Make the compiler error on unpaired value (i.e. no runtime effects)."
pub fn make_path_array<N: PathInteger, const LEN: usize>(list: &[N; LEN]) -> Result<Path64> {
    const {
        assert!(
            LEN % 2 == 0,
            "MakePath requires an even number of arguments"
        )
    };
    let mut result = Path64::new();
    make_path_generic(list, LEN, &mut result)?;
    Ok(result)
}

/// `template<typename T, ...arithmetic, not char> inline PathD MakePathD(const std::vector<T>&
/// list)`. An odd count throws (`DoError(non_pair_error_i)`).
pub fn make_path_d<N: PathNumber>(list: &[N]) -> Result<PathD> {
    let size = list.len() - list.len() % 2;
    if list.len() != size {
        return Err(do_error(NON_PAIR_ERROR_I)); // non-fatal without exception handling
    }
    let mut result = PathD::new();
    make_path_generic(list, size, &mut result)?;
    Ok(result)
}

/// `template<typename T, std::size_t N, ...arithmetic, not char> inline PathD MakePathD(const
/// T(&list)[N])`.
pub fn make_path_d_array<N: PathNumber, const LEN: usize>(list: &[N; LEN]) -> Result<PathD> {
    const {
        assert!(
            LEN % 2 == 0,
            "MakePath requires an even number of arguments"
        )
    };
    let mut result = PathD::new();
    make_path_generic(list, LEN, &mut result)?;
    Ok(result)
}

/// The `z_type` argument from a `T2` value: implicit conversion for an integral `T2`
/// (`MakePathZ`, and `MakePathZD`'s integer branch), `static_cast<int64_t>` otherwise.
fn z_from<N: PathNumber>(value: N) -> Result<i64> {
    match value.as_i64() {
        Some(z) => Ok(z),
        None => cast_f64_to_i64(value.as_f64(), "double to int64_t conversion out of range"),
    }
}

/// `template<typename T2, std::size_t N> inline Path64 MakePathZ(const T2(&list)[N])`
/// (`USINGZ`): integer triples `x, y, z`.
pub fn make_path_z<N: PathInteger, const LEN: usize>(list: &[N; LEN]) -> Result<Path64> {
    const {
        assert!(
            LEN % 3 == 0,
            "MakePathZ requires integer values in multiples of 3"
        )
    };
    let size = LEN / 3;
    let mut result = with_capacity(size)?;
    for i in 0..size {
        result.push(point_from::<i64, N>(
            *at(list, i * 3)?,
            *at(list, i * 3 + 1)?,
            z_from(*at(list, i * 3 + 2)?)?,
        )?);
    }
    Ok(result)
}

/// `template<typename T2, std::size_t N> inline PathD MakePathZD(const T2(&list)[N])`
/// (`USINGZ`): triples `x, y, z`; a floating `z` is converted by `static_cast<int64_t>`.
pub fn make_path_zd<N: PathNumber, const LEN: usize>(list: &[N; LEN]) -> Result<PathD> {
    const { assert!(LEN % 3 == 0, "MakePathZD requires values in multiples of 3") };
    let size = LEN / 3;
    let mut result = with_capacity(size)?;
    for i in 0..size {
        result.push(point_from::<f64, N>(
            *at(list, i * 3)?,
            *at(list, i * 3 + 1)?,
            z_from(*at(list, i * 3 + 2)?)?,
        )?);
    }
    Ok(result)
}

// --- path simplification -------------------------------------------------------------------

/// `inline Path64 TrimCollinear(const Path64& p, bool is_open_path = false)`. The reference's
/// iterators `srcIt`, `prevIt` and `stop` are indices into `p` with identical moves.
pub fn trim_collinear_64(p: &Path64, is_open_path: bool) -> Result<Path64> {
    let len = p.len();
    if len < 3 {
        if !is_open_path || len < 2 || at(p, 0)? == at(p, 1)? {
            return Ok(Path64::new());
        }
        return copy_of(p);
    }

    let mut dst = with_capacity(len)?;
    let mut src_it = 0usize;
    let mut stop = len - 1;

    if !is_open_path {
        while src_it != stop && is_collinear(at(p, stop)?, at(p, src_it)?, at(p, src_it + 1)?)? {
            src_it += 1;
        }
        while src_it != stop
            && is_collinear(at(p, stop.wrapping_sub(1))?, at(p, stop)?, at(p, src_it)?)?
        {
            stop -= 1;
        }
        if src_it == stop {
            return Ok(Path64::new());
        }
    }

    let mut prev_it = src_it;
    src_it += 1;
    dst.push(*at(p, prev_it)?);
    while src_it != stop {
        if !is_collinear(at(p, prev_it)?, at(p, src_it)?, at(p, src_it + 1)?)? {
            prev_it = src_it;
            dst.push(*at(p, prev_it)?);
        }
        src_it += 1;
    }

    if is_open_path {
        dst.push(*at(p, src_it)?);
    } else if !is_collinear(at(p, prev_it)?, at(p, stop)?, at(&dst, 0)?)? {
        dst.push(*at(p, stop)?);
    } else {
        while dst.len() > 2
            && is_collinear(
                at(&dst, dst.len() - 1)?,
                at(&dst, dst.len() - 2)?,
                at(&dst, 0)?,
            )?
        {
            dst.pop();
        }
        if dst.len() < 3 {
            return Ok(Path64::new());
        }
    }
    Ok(dst)
}

/// `inline PathD TrimCollinear(const PathD& path, int precision, bool is_open_path = false)`.
pub fn trim_collinear_d(path: &PathD, precision: i32, is_open_path: bool) -> Result<PathD> {
    let mut error_code = 0;
    let mut precision = precision;
    check_precision_range(&mut precision, &mut error_code)?;
    if error_code != 0 {
        return Ok(PathD::new());
    }
    let scale = pow10(precision);
    let mut p = scale_path::<i64, f64>(path, scale, &mut error_code)?;
    if error_code != 0 {
        return Ok(PathD::new());
    }
    p = trim_collinear_64(&p, is_open_path)?;
    scale_path::<f64, i64>(&p, 1.0 / scale, &mut error_code)
}

/// `template <typename T> inline double Distance(const Point<T> pt1, const Point<T> pt2)`.
pub fn distance<T: Coord>(pt1: &Point<T>, pt2: &Point<T>) -> Result<f64> {
    Ok(distance_sqr(pt1, pt2)?.sqrt())
}

/// `template <typename T> inline double Length(const Path<T>& path, bool is_closed_path =
/// false)`.
pub fn length<T: Coord>(path: &[Point<T>], is_closed_path: bool) -> Result<f64> {
    let mut result = 0.0;
    if path.len() < 2 {
        return Ok(result);
    }
    let stop = path.len() - 1;
    for it in 0..stop {
        result += distance(at(path, it)?, at(path, it + 1)?)?;
    }
    if is_closed_path {
        result += distance(at(path, stop)?, at(path, 0)?)?;
    }
    Ok(result)
}

/// `template <typename T> inline bool NearCollinear(const Point<T>& pt1, const Point<T>& pt2,
/// const Point<T>& pt3, double sin_sqrd_min_angle_rads)`.
pub fn near_collinear<T: Coord>(
    pt1: &Point<T>,
    pt2: &Point<T>,
    pt3: &Point<T>,
    sin_sqrd_min_angle_rads: f64,
) -> Result<bool> {
    let cp = cross_product3(pt1, pt2, pt3)?.abs();
    // The two `DistanceSqr` operands are unsequenced in the C++; both are pure, so either
    // order refuses the same inputs.
    let d12 = distance_sqr(pt1, pt2)?;
    let d23 = distance_sqr(pt2, pt3)?;
    Ok((cp * cp) / (d12 * d23) < sin_sqrd_min_angle_rads)
}

/// `template <typename T> inline Path<T> Ellipse(const Rect<T>& rect, size_t steps = 0)`:
/// `Ellipse(rect.MidPoint(), Width() * 0.5, Height() * 0.5, steps)`. The three arguments are
/// unsequenced in the C++ but pure, so any undefined one refuses the call.
pub fn ellipse_rect<T: Coord>(rect: &Rect<T>, steps: u64, max_points: usize) -> Result<Path<T>> {
    let center = rect.mid_point()?;
    let radius_x = rect.width()?.to_f64() * 0.5;
    let radius_y = rect.height()?.to_f64() * 0.5;
    ellipse(&center, radius_x, radius_y, steps, max_points)
}

/// `static_cast<size_t>(value)` on the 64-bit reference.
fn cast_size_t(value: f64) -> Result<u64> {
    if value.is_nan() || value <= -1.0 || value >= 18_446_744_073_709_551_616.0 {
        return Err(ClipperError::Undefined(
            "double to size_t conversion out of range",
        ));
    }
    Ok(value as u64)
}

/// `template <typename T> inline Path<T> Ellipse(const Point<T>& center, double radiusX,
/// double radiusY = 0, size_t steps = 0)`. The result holds `steps` points (computed from the
/// radii when `steps <= 2`); a count above the host's `max_points` is
/// [`ClipperError::ResourceLimit`] before anything is allocated.
pub fn ellipse<T: Coord>(
    center: &Point<T>,
    radius_x: f64,
    radius_y: f64,
    steps: u64,
    max_points: usize,
) -> Result<Path<T>> {
    let mut radius_y = radius_y;
    let mut steps = steps;
    if radius_x <= 0.0 {
        return Ok(Path::new());
    }
    if radius_y <= 0.0 {
        radius_y = radius_x;
    }
    if steps <= 2 {
        steps = cast_size_t(PI * ((radius_x + radius_y) / 2.0).sqrt())?;
    }

    let si = sin(2.0 * PI / steps as f64);
    let co = cos(2.0 * PI / steps as f64);
    let mut dx = co;
    let mut dy = si;
    // `reserve(steps)` then one point, plus one per further step.
    let count = usize::try_from(steps.max(1)).map_err(|_| ClipperError::ResourceLimit)?;
    if count > max_points {
        return Err(ClipperError::ResourceLimit);
    }
    let mut result = with_capacity(count)?;
    result.push(Point::<T>::new(
        center.x.to_f64() + radius_x,
        center.y.to_f64(),
        0,
    )?);
    for _ in 1..steps {
        result.push(Point::<T>::new(
            center.x.to_f64() + radius_x * dx,
            center.y.to_f64() + radius_y * dy,
            0,
        )?);
        let x = dx * co - dy * si;
        dy = dy * co + dx * si;
        dx = x;
    }
    Ok(result)
}

/// `details::GetNext(size_t current, size_t high, const std::vector<bool>& flags)`. Running
/// past the flags (every index flagged) is undefined and refused.
pub fn get_next(current: usize, high: usize, flags: &[bool]) -> Result<usize> {
    let mut current = current.wrapping_add(1);
    while current <= high && *at(flags, current)? {
        current += 1;
    }
    if current <= high {
        return Ok(current);
    }
    current = 0;
    while *at(flags, current)? {
        current += 1;
    }
    Ok(current)
}

/// `details::GetPrior(size_t current, size_t high, const std::vector<bool>& flags)`. Running
/// below index 0 (every index flagged) is undefined and refused.
pub fn get_prior(current: usize, high: usize, flags: &[bool]) -> Result<usize> {
    let mut current = if current == 0 { high } else { current - 1 };
    while current > 0 && *at(flags, current)? {
        current -= 1;
    }
    if !*at(flags, current)? {
        return Ok(current);
    }
    current = high;
    while *at(flags, current)? {
        current = current.wrapping_sub(1);
    }
    Ok(current)
}

/// `template <typename T> inline Path<T> SimplifyPath(const Path<T> &path, double epsilon,
/// bool isClosedPath = true)`.
///
/// Each pass of the main loop flags one more unflagged vertex, so it ends within `len`
/// passes; a pass beyond that (which the reference's invariants exclude) is refused instead
/// of looping.
pub fn simplify_path<T: Coord>(
    path: &[Point<T>],
    epsilon: f64,
    is_closed_path: bool,
) -> Result<Path<T>> {
    let len = path.len();
    let high = len.wrapping_sub(1);
    let eps_sqr = sqr(epsilon);
    if len < 4 {
        return copy_of(path);
    }

    let mut flags: Vec<bool> = with_capacity(len)?;
    flags.resize(len, false);
    let mut dist_sqr: Vec<f64> = with_capacity(len)?;
    dist_sqr.resize(len, 0.0);
    let mut curr: usize = 0;
    let (mut prior, mut next, mut prior2);
    if is_closed_path {
        set(
            &mut dist_sqr,
            0,
            perpendic_dist_from_line_sqrd(at(path, 0)?, at(path, high)?, at(path, 1)?)?,
        )?;
        set(
            &mut dist_sqr,
            high,
            perpendic_dist_from_line_sqrd(at(path, high)?, at(path, 0)?, at(path, high - 1)?)?,
        )?;
    } else {
        set(&mut dist_sqr, 0, MAX_DBL)?;
        set(&mut dist_sqr, high, MAX_DBL)?;
    }
    for i in 1..high {
        set(
            &mut dist_sqr,
            i,
            perpendic_dist_from_line_sqrd(at(path, i)?, at(path, i - 1)?, at(path, i + 1)?)?,
        )?;
    }

    let mut passes = 0usize;
    loop {
        passes += 1;
        if passes > len {
            return Err(ClipperError::Undefined("SimplifyPath loop does not end"));
        }
        if *at(&dist_sqr, curr)? > eps_sqr {
            let start = curr;
            loop {
                curr = get_next(curr, high, &flags)?;
                if !(curr != start && *at(&dist_sqr, curr)? > eps_sqr) {
                    break;
                }
            }
            if curr == start {
                break;
            }
        }

        prior = get_prior(curr, high, &flags)?;
        next = get_next(curr, high, &flags)?;
        if next == prior {
            break;
        }

        // flag for removal the smaller of adjacent 'distances'
        if *at(&dist_sqr, next)? < *at(&dist_sqr, curr)? {
            prior2 = prior;
            prior = curr;
            curr = next;
            next = get_next(next, high, &flags)?;
        } else {
            prior2 = get_prior(prior, high, &flags)?;
        }

        set(&mut flags, curr, true)?;
        curr = next;
        next = get_next(next, high, &flags)?;

        if is_closed_path || ((curr != high) && (curr != 0)) {
            let d =
                perpendic_dist_from_line_sqrd(at(path, curr)?, at(path, prior)?, at(path, next)?)?;
            set(&mut dist_sqr, curr, d)?;
        }
        if is_closed_path || ((prior != 0) && (prior != high)) {
            let d = perpendic_dist_from_line_sqrd(
                at(path, prior)?,
                at(path, prior2)?,
                at(path, curr)?,
            )?;
            set(&mut dist_sqr, prior, d)?;
        }
    }
    let mut result = with_capacity(len)?;
    for i in 0..len {
        if !*at(&flags, i)? {
            result.push(*at(path, i)?);
        }
    }
    Ok(result)
}

/// `template <typename T> inline Paths<T> SimplifyPaths(const Paths<T> &paths, double epsilon,
/// bool isClosedPath = true)`.
pub fn simplify_paths<T: Coord>(
    paths: &[Path<T>],
    epsilon: f64,
    is_closed_path: bool,
) -> Result<Paths<T>> {
    let mut result = with_capacity(paths.len())?;
    for path in paths {
        result.push(simplify_path(path, epsilon, is_closed_path)?);
    }
    Ok(result)
}

/// `template <typename T> inline bool Path2ContainsPath1(const Path<T>& path1,
/// const Path<T>& path2)`. "precondition: paths must not intersect, except for transient
/// (and presumed 'micro') path intersections".
pub fn path2_contains_path1<T: Coord>(path1: &[Point<T>], path2: &[Point<T>]) -> Result<bool> {
    let mut pip = PointInPolygonResult::IsOn;
    for pt in path1 {
        match point_in_polygon(pt, path2)? {
            PointInPolygonResult::IsOutside => {
                if pip == PointInPolygonResult::IsOutside {
                    return Ok(false);
                }
                pip = PointInPolygonResult::IsOutside;
            }
            PointInPolygonResult::IsInside => {
                if pip == PointInPolygonResult::IsInside {
                    return Ok(true);
                }
                pip = PointInPolygonResult::IsInside;
            }
            PointInPolygonResult::IsOn => {}
        }
    }
    if pip != PointInPolygonResult::IsInside {
        return Ok(false);
    }
    // result is likely true but check midpoint
    let mp1 = get_bounds_path::<T, T>(path1)?.mid_point()?;
    Ok(point_in_polygon(&mp1, path2)? == PointInPolygonResult::IsInside)
}

/// `template <typename T> inline void RDP(const Path<T> path, std::size_t begin,
/// std::size_t end, double epsSqrd, std::vector<bool>& flags)`.
///
/// The recursion runs on an explicit stack in the reference's call order (the `begin..idx`
/// call completes before the `idx..end` call starts). A call's control flow depends only on
/// its `begin` and `end` (it writes `flags` but never reads them), so a call whose arguments
/// repeat an active ancestor's recurses forever in the C++ (possible only with a NaN
/// `epsSqrd`); it is refused as undefined. The calls are also bounded by the host's
/// `max_calls` ([`ClipperError::ResourceLimit`]); a non-NaN `epsSqrd` makes at most
/// `2 * path.len() + 1` calls.
pub fn rdp<T: Coord>(
    path: &[Point<T>],
    begin: usize,
    end: usize,
    eps_sqrd: f64,
    flags: &mut [bool],
    max_calls: usize,
) -> Result<()> {
    // Pending calls: (begin, end, depth on the active chain).
    let mut pending: Vec<(usize, usize, usize)> = Vec::new();
    // The active chain of calls (arguments as entered) and a membership index over it.
    let mut chain: Vec<(usize, usize)> = Vec::new();
    let mut active = ::std::collections::BTreeSet::new();
    let mut calls = 0usize;
    push(&mut pending, (begin, end, 0))?;
    while let Some((begin, end_in, depth)) = pending.pop() {
        calls += 1;
        if calls > max_calls {
            return Err(ClipperError::ResourceLimit);
        }
        // Return from every call deeper than this one's parent.
        while chain.len() > depth {
            if let Some(done) = chain.pop() {
                active.remove(&done);
            }
        }
        if !active.insert((begin, end_in)) {
            return Err(ClipperError::Undefined("RDP recursion does not end"));
        }
        push(&mut chain, (begin, end_in))?;

        let mut end = end_in;
        let mut idx = 0usize;
        let mut max_d = 0.0;
        while end > begin && at(path, begin)? == at(path, end)? {
            set(flags, end, false)?;
            end -= 1;
        }
        for i in begin.wrapping_add(1)..end {
            // PerpendicDistFromLineSqrd - avoids expensive Sqrt()
            let d = perpendic_dist_from_line_sqrd(at(path, i)?, at(path, begin)?, at(path, end)?)?;
            if d <= max_d {
                continue;
            }
            max_d = d;
            idx = i;
        }
        if max_d <= eps_sqrd {
            continue;
        }
        set(flags, idx, true)?;
        // Pushed in reverse so that `RDP(path, begin, idx, ...)` runs first.
        if idx < end.wrapping_sub(1) {
            push(&mut pending, (idx, end, depth + 1))?;
        }
        if idx > begin.wrapping_add(1) {
            push(&mut pending, (begin, idx, depth + 1))?;
        }
    }
    Ok(())
}

/// `template <typename T> inline Path<T> RamerDouglasPeucker(const Path<T>& path,
/// double epsilon)`. `RDP`'s calls are bounded by `max_calls` (see [`rdp`]).
pub fn ramer_douglas_peucker<T: Coord>(
    path: &[Point<T>],
    epsilon: f64,
    max_calls: usize,
) -> Result<Path<T>> {
    let len = path.len();
    if len < 5 {
        return copy_of(path);
    }
    let mut flags: Vec<bool> = with_capacity(len)?;
    flags.resize(len, false);
    set(&mut flags, 0, true)?;
    set(&mut flags, len - 1, true)?;
    rdp(path, 0, len - 1, sqr(epsilon), &mut flags, max_calls)?;
    let mut result = with_capacity(len)?;
    for i in 0..len {
        if *at(&flags, i)? {
            result.push(*at(path, i)?);
        }
    }
    Ok(result)
}

/// `template <typename T> inline Paths<T> RamerDouglasPeucker(const Paths<T>& paths,
/// double epsilon)`, each path's `RDP` calls bounded by `max_calls`.
pub fn ramer_douglas_peucker_paths<T: Coord>(
    paths: &[Path<T>],
    epsilon: f64,
    max_calls: usize,
) -> Result<Paths<T>> {
    let mut result = with_capacity(paths.len())?;
    for path in paths {
        result.push(ramer_douglas_peucker(path, epsilon, max_calls)?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
