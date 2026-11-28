// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/line.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `LINE` (libs/kimath/include/geometry/line.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7: the type and its inline members, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! It stores one [`Seg`] (`m_seg`) through which the line passes. The out-of-line members (both
//! `Intersect`, `Distance`, `NearestPoint`) are line.cpp's
//! (`libs.kimath.src.geometry.line.application.ui.p001`).
use crate::seg_header::Seg;
use crate::vector::Vector2I;

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.pure.p001";

/// `LINE::ecoord`: `VECTOR2I::extended_type`.
pub type LineEcoord = i64;

/// `LINE`: an infinite line through `m_seg`. `Copy`/`Clone` are the implicit copy operations
/// (the segment's index included).
#[derive(Clone, Copy, Debug)]
pub struct Line {
    seg: Seg,
}

impl PartialEq for Line {
    /// `operator==`: `m_seg == aOther.m_seg`.
    fn eq(&self, other: &Self) -> bool {
        self.seg == other.seg
    }
}

impl Line {
    /// `LINE( const SEG& aSeg )`: `m_seg( aSeg )`.
    pub const fn from_seg(seg: Seg) -> Self {
        Self { seg }
    }

    /// `LINE( const VECTOR2I& aStart, const VECTOR2I& aEnd )`: `m_seg( aStart, aEnd )`.
    pub const fn new(start: Vector2I, end: Vector2I) -> Self {
        Self {
            seg: Seg::new(start, end),
        }
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
        let l = Line::from_seg(s);
        assert_eq!(l.get_contained_seg().index(), 7);
        assert_eq!(l, Line::new(Vector2::new(1, 2), Vector2::new(3, 4)));
        assert!(l != Line::new(Vector2::new(3, 4), Vector2::new(1, 2)));
    }
}
