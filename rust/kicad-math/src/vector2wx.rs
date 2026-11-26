// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! Port of `libs/kimath/include/math/vector2wx.h` at KiCad pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by unit
//! `libs.kimath.include.math.application.thin.replacement.p001`.
//!
//! The header converts between wxWidgets' integer `wxPoint`/`wxSize` and KiCad's
//! `VECTOR2I`/`VECTOR2D`. This crate is portable and has no GUI dependency, so
//! [`WxPoint`] and [`WxSize`] are minimal plain value stand-ins for the two wx host
//! boundary types: exactly the public `int x, y` members and the zero default
//! constructor of wx 3.2 `wx/gdicmn.h`, and nothing else. No other wx behaviour
//! (arithmetic, `wxDefaultCoord` handling, `IsFullySpecified`, ...) is modelled. A
//! host adapter maps them to and from the real wx objects field for field.
//!
//! Every conversion is a component copy: `int` to `int` is exact, and `int` to
//! `double` is exact for every 32-bit value, so no rounding, saturation or error
//! is possible and the functions are infallible.

use crate::vector::{Vector2D, Vector2I};

/// Plain value stand-in for wx 3.2 `wxPoint` (`int x, y`; `wxPoint()` is `(0, 0)`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WxPoint {
    pub x: i32,
    pub y: i32,
}

/// Plain value stand-in for wx 3.2 `wxSize` (`int x, y`; `wxSize()` is `(0, 0)`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WxSize {
    pub x: i32,
    pub y: i32,
}

impl WxPoint {
    /// `wxPoint( int xx, int yy )`
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl WxSize {
    /// `wxSize( int xx, int yy )`
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// `VECTOR2I ToVECTOR2I( const wxSize& aSize )`: `{ aSize.x, aSize.y }`.
pub const fn to_vector2i_from_size(size: &WxSize) -> Vector2I {
    Vector2I::new(size.x, size.y)
}

/// `VECTOR2I ToVECTOR2I( const wxPoint& aSize )`: `{ aSize.x, aSize.y }`.
pub const fn to_vector2i_from_point(point: &WxPoint) -> Vector2I {
    Vector2I::new(point.x, point.y)
}

/// `VECTOR2D ToVECTOR2D( const wxPoint& aPoint )`: `VECTOR2D( aPoint.x, aPoint.y )`,
/// each `int` converted exactly to `double`.
pub fn to_vector2d_from_point(point: &WxPoint) -> Vector2D {
    Vector2D::new(f64::from(point.x), f64::from(point.y))
}

/// `VECTOR2D ToVECTOR2D( const wxSize& aPoint )`: `VECTOR2D( aPoint.x, aPoint.y )`,
/// each `int` converted exactly to `double`.
pub fn to_vector2d_from_size(size: &WxSize) -> Vector2D {
    Vector2D::new(f64::from(size.x), f64::from(size.y))
}

/// `wxPoint ToWxPoint( const VECTOR2I& aSize )`: `wxPoint( aSize.x, aSize.y )`.
pub const fn to_wx_point(vector: &Vector2I) -> WxPoint {
    WxPoint::new(vector.x, vector.y)
}

/// `wxSize ToWxSize( const VECTOR2I& aSize )`: `wxSize( aSize.x, aSize.y )`.
pub const fn to_wx_size(vector: &Vector2I) -> WxSize {
    WxSize::new(vector.x, vector.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDGES: [i32; 7] = [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX];

    #[test]
    fn layout_and_defaults_match_wx() {
        // wx 3.2: two `int` members, x first; the adapter static_asserts the same.
        assert_eq!(core::mem::size_of::<WxPoint>(), 8);
        assert_eq!(core::mem::size_of::<WxSize>(), 8);
        assert_eq!(core::mem::align_of::<WxPoint>(), 4);
        assert_eq!(WxPoint::default(), WxPoint::new(0, 0));
        assert_eq!(WxSize::default(), WxSize::new(0, 0));
    }

    #[test]
    fn conversions_copy_components_exactly_and_round_trip() {
        for x in EDGES {
            for y in EDGES {
                let v = to_vector2i_from_point(&WxPoint::new(x, y));
                assert_eq!((v.x, v.y), (x, y));
                let v = to_vector2i_from_size(&WxSize::new(x, y));
                assert_eq!((v.x, v.y), (x, y));
                let d = to_vector2d_from_point(&WxPoint::new(x, y));
                assert_eq!((d.x as i32, d.y as i32), (x, y));
                assert_eq!(d.x.to_bits(), f64::from(x).to_bits());
                let d = to_vector2d_from_size(&WxSize::new(x, y));
                assert_eq!(d.y.to_bits(), f64::from(y).to_bits());
                assert_eq!(to_wx_point(&Vector2I::new(x, y)), WxPoint::new(x, y));
                assert_eq!(to_wx_size(&Vector2I::new(x, y)), WxSize::new(x, y));
            }
        }
        // Zero converts to +0.0, never -0.0.
        assert_eq!(to_vector2d_from_point(&WxPoint::default()).x.to_bits(), 0);
    }
}
