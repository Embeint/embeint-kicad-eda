// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! `std::less<wxPoint>` (`libs/core/wx_stl_compat.cpp:31-39`).
//!
//! The header comments that it is "Required to use wxPoint as key type in maps"; the body is
//! compiled because `core/wx_stl_compat.h` defines `USE_KICAD_WXPOINT_LESS_AND_HASH` before
//! declaring it. The matching `std::hash<wxPoint>::operator()` is declared `= delete`, so
//! there is no hash to port.

use core::cmp::Ordering;

use crate::WxPoint;

/// `bool std::less<wxPoint>::operator()( const wxPoint& aA, const wxPoint& aB ) const`:
/// `aA.y < aB.y` when the `x` members are equal, otherwise `aA.x < aB.x`.
pub const fn less_wx_point(a: &WxPoint, b: &WxPoint) -> bool {
    if a.x == b.x {
        return a.y < b.y;
    }

    a.x < b.x
}

/// A `wxPoint` ordered by `std::less<wxPoint>`, for use as a `BTreeMap` key in place of a
/// `std::map<wxPoint, T>` key.
///
/// The order is derived from [`less_wx_point`] alone, as `std::map` derives equivalence from
/// `std::less`: neither point is less than the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WxPointKey(pub WxPoint);

impl Ord for WxPointKey {
    fn cmp(&self, other: &Self) -> Ordering {
        if less_wx_point(&self.0, &other.0) {
            Ordering::Less
        } else if less_wx_point(&other.0, &self.0) {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

impl PartialOrd for WxPointKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_by_x_then_y() {
        let p = |x, y| WxPoint::new(x, y);
        assert!(less_wx_point(&p(0, 5), &p(1, -5)));
        assert!(!less_wx_point(&p(1, -5), &p(0, 5)));
        assert!(less_wx_point(&p(3, -1), &p(3, 0)));
        assert!(!less_wx_point(&p(3, 0), &p(3, 0)));
        assert!(less_wx_point(
            &p(i32::MIN, i32::MAX),
            &p(i32::MAX, i32::MIN)
        ));
        assert_eq!(
            WxPointKey(p(2, 2)).cmp(&WxPointKey(p(2, 2))),
            Ordering::Equal
        );
    }
}
