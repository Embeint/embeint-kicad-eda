// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kimath/include/transform.h`: the `TRANSFORM` members this unit owns
//! (both constructors and `operator!=`).
//!
//! `operator!=` is `!( *this == aTransform )`. Its callee
//! `TRANSFORM::operator==( const TRANSFORM& ) const`
//! (`c:@S@TRANSFORM@F@operator==#&1$@S@TRANSFORM#1`, `libs/kimath/src/transform.cpp`)
//! belongs to `libs.kimath.src.transform.application.ui.p001`; it is co-ported
//! here, by parent agreement, as the minimum needed for the owned inequality.
//! That unit keeps ownership of it and of `TransformCoordinate`,
//! `InverseTransform`, `MapAngles` and `std::hash<TRANSFORM>`, which are not
//! implemented here.

/// `TRANSFORM`: a 2x2 integer mirror/rotation matrix for symbol coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

impl Default for Transform {
    /// `TRANSFORM()`: the normal orientation `x1( 1 ), y1( 0 ), x2( 0 ), y2( 1 )`.
    fn default() -> Self {
        Self {
            x1: 1,
            y1: 0,
            x2: 0,
            y2: 1,
        }
    }
}

impl Transform {
    /// `TRANSFORM( int ax1, int ay1, int ax2, int ay2 )`.
    pub fn new(x1: i32, y1: i32, x2: i32, y2: i32) -> Self {
        Self { x1, y1, x2, y2 }
    }
}

impl PartialEq for Transform {
    /// Co-port of `TRANSFORM::operator==` (owner
    /// `libs.kimath.src.transform.application.ui.p001`): all four members equal,
    /// compared in the order `x1`, `y1`, `x2`, `y2`.
    ///
    /// The owned `operator!=` is the provided `PartialEq::ne`, which is exactly
    /// `!( *this == aTransform )`.
    fn eq(&self, other: &Self) -> bool {
        self.x1 == other.x1 && self.y1 == other.y1 && self.x2 == other.x2 && self.y2 == other.y2
    }
}

impl Eq for Transform {}
