// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/seg.h.
// Copyright (C) 2013 CERN, author Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `SEG` type and inline bodies (libs/kimath/include/geometry/seg.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.ui.p003`.
//!
//! The members, constructors, `operator==` and `Index` were first co-ported by the seg.cpp unit
//! (PR #74); this unit adopts them unchanged and adds every other inline body. The out-of-line
//! seg.cpp bodies (`Contains( VECTOR2I )`, `Intersect`, `ApproxCollinear`, ...) stay in
//! [`crate::seg`] and are called from here as the header calls them.
//!
//! Signed `int` and `int64_t` overflow and `std::abs` of the minimum are undefined in the C++ and
//! are refused with [`SegError::Undefined`] at the first such operation.
use crate::seg::{Ecoord, OptVector2I, SegError, SegResult};
use crate::stream::{insert_str, StreamError, StreamInsert, StreamState};
use crate::vector::{Vector2, Vector2I};

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.ui.p003";

fn checked<T>(value: Option<T>) -> SegResult<T> {
    value.ok_or(SegError::Undefined)
}

/// `SEG`: a segment from `a` to `b`, with the index it has within a parent shape.
///
/// `Copy`/`Clone` are the user-declared copy constructor and copy assignment, which copy all
/// three members (the index included).
#[derive(Clone, Copy, Debug)]
pub struct Seg {
    pub a: Vector2I,
    pub b: Vector2I,
    index: i32,
}

impl Default for Seg {
    /// `SEG()`: `A` and `B` value-initialised to (0, 0), index -1.
    fn default() -> Self {
        Self::new(Vector2::default(), Vector2::default())
    }
}

impl PartialEq for Seg {
    /// `SEG::operator==` compares the points only, never the index. `operator!=`
    /// (`A != aSeg.A || B != aSeg.B`) is its exact negation for integer points.
    fn eq(&self, other: &Self) -> bool {
        self.a == other.a && self.b == other.b
    }
}

impl Seg {
    /// `SEG( const VECTOR2I& aA, const VECTOR2I& aB )`: index -1.
    pub const fn new(a: Vector2I, b: Vector2I) -> Self {
        Self { a, b, index: -1 }
    }

    /// `SEG( int aX1, int aY1, int aX2, int aY2 )`: index -1.
    pub const fn from_coords(x1: i32, y1: i32, x2: i32, y2: i32) -> Self {
        Self::new(Vector2::new(x1, y1), Vector2::new(x2, y2))
    }

    /// `SEG( const VECTOR2I& aA, const VECTOR2I& aB, int aIndex )`.
    pub const fn with_index(a: Vector2I, b: Vector2I, index: i32) -> Self {
        Self { a, b, index }
    }

    /// `SEG::Index`.
    pub const fn index(&self) -> i32 {
        self.index
    }

    /// `static SEG::Square( int a )`: `ecoord( a ) * a`, which always fits `int64_t`.
    pub const fn square(a: i32) -> Ecoord {
        a as i64 * a as i64
    }

    /// `SEG::Side`: the sign of `( B - A ).Cross( aP - A )`; both differences are `int`.
    pub fn side(&self, p: Vector2I) -> SegResult<i32> {
        let dir = self.b.try_sub(self.a)?;
        let det = dir.cross(p.try_sub(self.a)?)?;
        Ok(if det < 0 {
            -1
        } else if det > 0 {
            1
        } else {
            0
        })
    }

    /// `SEG::IntersectLines`: `Intersect( aSeg, false, true )`.
    pub fn intersect_lines(&self, seg: &Seg) -> SegResult<OptVector2I> {
        self.intersect(seg, false, true)
    }

    /// `SEG::CanonicalCoefs` as `(qA, qB, qC)`: `qA = A.y - B.y`, `qB = B.x - A.x` (both
    /// exact in `int64_t`), `qC = -qA * A.x - qB * A.y`.
    pub fn canonical_coefs(&self) -> SegResult<(Ecoord, Ecoord, Ecoord)> {
        let qa = i64::from(self.a.y) - i64::from(self.b.y);
        let qb = i64::from(self.b.x) - i64::from(self.a.x);
        // Each operation is checked as written. In value qC = A.x * B.y - A.y * B.x, so with
        // 32-bit points none of them can actually leave the int64_t range.
        let left = checked((-qa).checked_mul(i64::from(self.a.x)))?;
        let right = checked(qb.checked_mul(i64::from(self.a.y)))?;
        let qc = checked(left.checked_sub(right))?;
        Ok((qa, qb, qc))
    }

    /// `SEG::Collinear`: both ends of `seg` within 1 of this segment's canonical line value.
    pub fn collinear(&self, seg: &Seg) -> SegResult<bool> {
        let (qa, qb, qc) = self.canonical_coefs()?;
        // `aSeg.A.x * qa + aSeg.A.y * qb + qc` in int64_t, then std::abs; d1 and d2 are both
        // computed before the comparison.
        let line = |p: Vector2I| -> SegResult<Ecoord> {
            let x = checked(i64::from(p.x).checked_mul(qa))?;
            let y = checked(i64::from(p.y).checked_mul(qb))?;
            let sum = checked(checked(x.checked_add(y))?.checked_add(qc))?;
            checked(sum.checked_abs())
        };
        let d1 = line(seg.a)?;
        let d2 = line(seg.b)?;
        Ok(d1 <= 1 && d2 <= 1)
    }

    /// `SEG::Overlaps`, with `Contains`'s default threshold of 3.
    pub fn overlaps(&self, seg: &Seg) -> SegResult<bool> {
        if seg.a == seg.b {
            // single point corner case
            if self.a == seg.a || self.b == seg.a {
                return Ok(false);
            }
            return self.contains_point(seg.a, 3);
        }
        if !self.collinear(seg)? {
            return Ok(false);
        }
        if self.contains_point(seg.a, 3)? || self.contains_point(seg.b, 3)? {
            return Ok(true);
        }
        if seg.contains_point(self.a, 3)? || seg.contains_point(self.b, 3)? {
            return Ok(true);
        }
        Ok(false)
    }

    /// `SEG::ApproxOverlaps`; the C++ default threshold is 3.
    pub fn approx_overlaps(&self, seg: &Seg, threshold: i32) -> SegResult<bool> {
        if self.a == self.b {
            // single point corner case
            if seg.a == self.a || seg.b == self.a {
                return Ok(false);
            }
            return seg.contains_point(self.a, threshold);
        }
        if !seg.approx_collinear(self, threshold)? {
            return Ok(false);
        }
        if seg.contains_point(self.a, threshold)? || seg.contains_point(self.b, threshold)? {
            return Ok(true);
        }
        if self.contains_point(seg.a, threshold)? || self.contains_point(seg.b, threshold)? {
            return Ok(true);
        }
        Ok(false)
    }

    /// `SEG::Contains( const SEG& )`, with `Contains( VECTOR2I )`'s default threshold of 3.
    pub fn contains_seg(&self, seg: &Seg) -> SegResult<bool> {
        if seg.a == seg.b {
            // single point corner case
            return self.contains_point(seg.a, 3);
        }
        if !self.collinear(seg)? {
            return Ok(false);
        }
        if self.contains_point(seg.a, 3)? && self.contains_point(seg.b, 3)? {
            return Ok(true);
        }
        Ok(false)
    }

    /// `SEG::Length`: `( A - B ).EuclideanNorm()`, the rounded hypotenuse capped at `INT_MAX`.
    pub fn length(&self) -> SegResult<i32> {
        Ok(self.a.try_sub(self.b)?.euclidean_norm()?)
    }

    /// `SEG::SquaredLength`: `( A - B ).SquaredEuclideanNorm()`, which saturates at
    /// `ECOORD_MAX`; only the `int` difference can be undefined.
    pub fn squared_length(&self) -> SegResult<Ecoord> {
        Ok(self.a.try_sub(self.b)?.squared_euclidean_norm()?)
    }

    /// `SEG::TCoef`: `( B - A ).Dot( aP - A )`.
    pub fn tcoef(&self, p: Vector2I) -> SegResult<Ecoord> {
        let d = self.b.try_sub(self.a)?;
        Ok(d.dot(p.try_sub(self.a)?)?)
    }

    /// `SEG::Reverse`: swaps `A` and `B`; the index is kept.
    pub fn reverse(&mut self) {
        core::mem::swap(&mut self.a, &mut self.b);
    }

    /// `SEG::Reversed`: `SEG( B, A )`, so the index becomes -1.
    pub const fn reversed(&self) -> Seg {
        Seg::new(self.b, self.a)
    }

    /// `SEG::Center`: `A + ( B - A ) / 2`. The divisor converts to `double` (the only
    /// `VECTOR2::operator/`), so each half is `KiROUND( x / 2.0 )`, rounding halves away from
    /// zero.
    pub fn center(&self) -> SegResult<Vector2I> {
        let half = self.b.try_sub(self.a)?.divide(2.0)?;
        Ok(self.a.try_add(half)?)
    }

    /// `SEG::operator<`: by `A`, then by `B`, with `VECTOR2`'s lexicographic `operator<`.
    pub fn less(&self, seg: &Seg) -> bool {
        if self.a == seg.a {
            return self.b.less(&seg.b);
        }
        self.a.less(&seg.a)
    }
}

/// `operator<<( std::ostream& aStream, const SEG& aSeg )`:
/// `aStream << "[ " << aSeg.A << " - " << aSeg.B << " ]"` into the caller's stream. The format
/// flags (base, `showbase`, `showpos`, `uppercase`, adjustment) reach every coordinate through
/// `VECTOR2<int>`'s `operator<<`; the width pads only the first insertion, `"[ "`, which resets
/// it to 0, as do all later insertions.
impl StreamInsert for Seg {
    fn insert(&self, state: &mut StreamState) -> Result<String, StreamError> {
        let mut out = insert_str("[ ", state)?;
        out.push_str(&self.a.insert(state)?);
        out.push_str(&insert_str(" - ", state)?);
        out.push_str(&self.b.insert(state)?);
        out.push_str(&insert_str(" ]", state)?);
        Ok(out)
    }
}
