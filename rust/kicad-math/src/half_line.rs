// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/half_line.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `HALF_LINE` (libs/kimath/include/geometry/half_line.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7: the type and its inline members, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! It stores one [`Seg`] (`m_seg`) through which the ray passes. The out-of-line members
//! (`Contains`, both `Intersect`, `NearestPoint`) and `ClipHalfLineToBox` are half_line.cpp's
//! (`libs.kimath.src.geometry.half_line.application.ui.p001`).
use crate::seg_header::Seg;
use crate::vector::Vector2I;

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.pure.p001";

/// `HALF_LINE`: a ray from `m_seg.A` through `m_seg.B`. `Copy`/`Clone` are the implicit copy
/// operations (the segment's index included).
#[derive(Clone, Copy, Debug)]
pub struct HalfLine {
    seg: Seg,
}

impl PartialEq for HalfLine {
    /// `operator==`: `m_seg == aOther.m_seg` (`SEG::operator==`, points only).
    fn eq(&self, other: &Self) -> bool {
        self.seg == other.seg
    }
}

impl HalfLine {
    /// `HALF_LINE( const SEG& aSeg )`: `m_seg( aSeg )`, a copy (index kept).
    pub const fn from_seg(seg: Seg) -> Self {
        Self { seg }
    }

    /// `HALF_LINE( const VECTOR2I& aStart, const VECTOR2I& aOtherContainedPoint )`:
    /// `m_seg( aStart, aOtherContainedPoint )` (index -1).
    pub const fn new(start: Vector2I, other_contained_point: Vector2I) -> Self {
        Self {
            seg: Seg::new(start, other_contained_point),
        }
    }

    /// `GetStart`: `m_seg.A`.
    pub const fn get_start(&self) -> &Vector2I {
        &self.seg.a
    }

    /// `GetContainedPoint`: `m_seg.B`.
    pub const fn get_contained_point(&self) -> &Vector2I {
        &self.seg.b
    }

    /// `GetContainedSeg`: `m_seg`.
    pub const fn get_contained_seg(&self) -> &Seg {
        &self.seg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::Vector2;

    #[test]
    fn members_copy_the_segment() {
        let s = Seg::with_index(Vector2::new(1, 2), Vector2::new(3, 4), 7);
        let h = HalfLine::from_seg(s);
        assert_eq!(h.get_contained_seg().index(), 7);
        assert_eq!(*h.get_start(), Vector2::new(1, 2));
        assert_eq!(*h.get_contained_point(), Vector2::new(3, 4));
        assert_eq!(h, HalfLine::new(Vector2::new(1, 2), Vector2::new(3, 4)));
        assert_eq!(
            HalfLine::new(Vector2::new(1, 2), Vector2::new(3, 4))
                .get_contained_seg()
                .index(),
            -1
        );
    }
}
