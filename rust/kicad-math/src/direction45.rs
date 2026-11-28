// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/direction45.h.
// Copyright (C) 2013-2015 CERN, author Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `DIRECTION_45` (libs/kimath/include/geometry/direction45.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! Every inline member is ported in header order. `BuildInitialTrace` is out-of-line
//! (direction_45.cpp) and belongs to `libs.kimath.src.geometry.direction_45.application.ui.p001`.
//!
//! `Directions` has the fixed underlying type `int`, so a `DIRECTION_45` can hold any `int`
//! (`DIRECTION_45( (Directions) k )`); the members are defined for every such value except where
//! the C++ evaluation is undefined, which returns [`Direction45Error::Undefined`]: the negation of
//! `INT_MIN` and the `int` difference of the constructors, an `OppositeMap` index outside
//! `0..=8`, a shift count outside `0..=31` in `Mask`, the `int` overflow of `Angle`, `Right` and
//! `Left`, and `std::abs( INT_MIN )` in `Angle`.
//!
//! `construct_` calls `atan2` through [`crate::reference_libm`] (glibc 2.39). Its `double`
//! arithmetic never sees a NaN (both arguments are finite and not both zero), so the IEEE
//! results of the source-order operations are the reference bits.
use crate::reference_libm;
use crate::seg_header::Seg;
use crate::shape_arc::ShapeArc;
use crate::vector::{Vector2, Vector2I};

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.pure.p001";

/// The pinned C++ evaluation is undefined for this input; no value exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction45Error {
    Undefined,
}

pub type Direction45Result<T> = Result<T, Direction45Error>;

/// `DIRECTION_45::Directions`: an `int` (the enum's fixed underlying type).
pub type Directions = i32;

/// `DIRECTION_45::CORNER_MODE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum CornerMode {
    /// H/V/45 with mitered corners (default).
    Mitered45 = 0,
    /// H/V/45 with filleted corners.
    Rounded45 = 1,
    /// H/V only (90-degree corners).
    Mitered90 = 2,
    /// H/V with filleted corners.
    Rounded90 = 3,
}

/// `DIRECTION_45::AngleType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum AngleType {
    AngObtuse = 0x01,
    AngRight = 0x02,
    AngAcute = 0x04,
    AngStraight = 0x08,
    AngHalfFull = 0x10,
    AngUndefined = 0x20,
}

/// `DIRECTION_45`: a direction in 45 degree steps (`m_dir`) and whether routing uses 90 degree
/// increments (`m_90deg`). `Copy`/`Clone` are the implicit copy operations.
#[derive(Clone, Copy, Debug)]
pub struct Direction45 {
    dir: Directions,
    deg90: bool,
}

impl Default for Direction45 {
    /// `DIRECTION_45( Directions aDir = UNDEFINED )`.
    fn default() -> Self {
        Self::new(Self::UNDEFINED)
    }
}

impl PartialEq for Direction45 {
    /// `operator==`: `aOther.m_dir == m_dir` (the 90 degree flag is not compared).
    /// `operator!=` (`aOther.m_dir != m_dir`) is its exact negation.
    fn eq(&self, other: &Self) -> bool {
        other.dir == self.dir
    }
}

impl Direction45 {
    pub const N: Directions = 0;
    pub const NE: Directions = 1;
    pub const E: Directions = 2;
    pub const SE: Directions = 3;
    pub const S: Directions = 4;
    pub const SW: Directions = 5;
    pub const W: Directions = 6;
    pub const NW: Directions = 7;
    pub const LAST: Directions = 8;
    pub const UNDEFINED: Directions = -1;

    /// `DIRECTION_45( Directions aDir )`: `m_dir( aDir )`, `m_90deg( false )`.
    pub const fn new(dir: Directions) -> Self {
        Self { dir, deg90: false }
    }

    /// `DIRECTION_45( const VECTOR2I& aVec, bool a90 = false )`: the vector with `y` negated.
    pub fn from_vector(vec: Vector2I, deg90: bool) -> Direction45Result<Self> {
        let mut d = Self { dir: 0, deg90 };
        let y = vec.y.checked_neg().ok_or(Direction45Error::Undefined)?;
        d.construct(Vector2::new(vec.x, y));
        Ok(d)
    }

    /// `DIRECTION_45( const SEG& aSeg, bool a90 = false )`: `aSeg.B - aSeg.A`, `y` negated.
    pub fn from_seg(seg: &Seg, deg90: bool) -> Direction45Result<Self> {
        let vec = seg
            .b
            .checked_sub(seg.a)
            .map_err(|_| Direction45Error::Undefined)?;
        Self::from_vector(vec, deg90)
    }

    /// `DIRECTION_45( const SHAPE_ARC& aArc, bool a90 = false )`: `GetP1() - GetP0()`, `y`
    /// negated.
    pub fn from_arc(arc: &ShapeArc, deg90: bool) -> Direction45Result<Self> {
        let vec = arc
            .get_p1()
            .checked_sub(arc.get_p0())
            .map_err(|_| Direction45Error::Undefined)?;
        Self::from_vector(vec, deg90)
    }

    /// `m_dir` (private in C++; observed by the adapters).
    pub const fn dir(&self) -> Directions {
        self.dir
    }

    /// `m_90deg` (private in C++; observed by the adapters).
    pub const fn is_90deg(&self) -> bool {
        self.deg90
    }

    /// `Format`.
    pub const fn format(&self) -> &'static str {
        match self.dir {
            Self::N => "north",
            Self::NE => "north-east",
            Self::E => "east",
            Self::SE => "south-east",
            Self::S => "south",
            Self::SW => "south-west",
            Self::W => "west",
            Self::NW => "north-west",
            Self::UNDEFINED => "undefined",
            _ => "<Error>",
        }
    }

    /// `Opposite`: `OppositeMap[m_dir]`; the result is constructed from `Directions` (so its
    /// 90 degree flag is false). An index outside the nine-element map is undefined.
    pub fn opposite(&self) -> Direction45Result<Self> {
        const OPPOSITE_MAP: [Directions; 9] = [
            Direction45::S,
            Direction45::SW,
            Direction45::W,
            Direction45::NW,
            Direction45::N,
            Direction45::NE,
            Direction45::E,
            Direction45::SE,
            Direction45::UNDEFINED,
        ];
        let index = usize::try_from(self.dir).map_err(|_| Direction45Error::Undefined)?;
        OPPOSITE_MAP
            .get(index)
            .map(|d| Self::new(*d))
            .ok_or(Direction45Error::Undefined)
    }

    /// `Angle`.
    pub fn angle(&self, other: &Self) -> Direction45Result<AngleType> {
        if self.dir == Self::UNDEFINED || other.dir == Self::UNDEFINED {
            return Ok(AngleType::AngUndefined);
        }

        let d = self
            .dir
            .checked_sub(other.dir)
            .and_then(i32::checked_abs)
            .ok_or(Direction45Error::Undefined)?;

        Ok(if d == 1 || d == 7 {
            AngleType::AngObtuse
        } else if d == 2 || d == 6 {
            AngleType::AngRight
        } else if d == 3 || d == 5 {
            AngleType::AngAcute
        } else if d == 4 {
            AngleType::AngHalfFull
        } else {
            AngleType::AngStraight
        })
    }

    /// `IsObtuse`.
    pub fn is_obtuse(&self, other: &Self) -> Direction45Result<bool> {
        Ok(self.angle(other)? == AngleType::AngObtuse)
    }

    /// `IsDiagonal`: `( m_dir % 2 ) == 1` (C++ remainder truncates toward zero, as Rust's).
    pub const fn is_diagonal(&self) -> bool {
        (self.dir % 2) == 1
    }

    /// `IsDefined`.
    pub const fn is_defined(&self) -> bool {
        self.dir != Self::UNDEFINED
    }

    /// `Right`: a default-constructed result (`UNDEFINED`, 90 degree flag false) whose
    /// direction is set when this one is defined.
    pub fn right(&self) -> Direction45Result<Self> {
        let mut r = Self::default();

        if self.dir != Self::UNDEFINED {
            let step = if self.deg90 { 2 } else { 1 };
            r.dir = self
                .dir
                .checked_add(step)
                .ok_or(Direction45Error::Undefined)?
                % Self::LAST;
        }

        Ok(r)
    }

    /// `Left`: as [`Self::right`] with `( m_dir + LAST - 2 )` or `( m_dir + LAST - 1 )`,
    /// evaluated left to right.
    pub fn left(&self) -> Direction45Result<Self> {
        let mut l = Self::default();

        if self.dir != Self::UNDEFINED {
            let step = if self.deg90 { 2 } else { 1 };
            l.dir = self
                .dir
                .checked_add(Self::LAST)
                .and_then(|v| v.checked_sub(step))
                .ok_or(Direction45Error::Undefined)?
                % Self::LAST;
        }

        Ok(l)
    }

    /// `ToVector`.
    pub const fn to_vector(&self) -> Vector2I {
        match self.dir {
            Self::N => Vector2::new(0, -1),
            Self::S => Vector2::new(0, 1),
            Self::E => Vector2::new(1, 0),
            Self::W => Vector2::new(-1, 0),
            Self::NE => Vector2::new(1, -1),
            Self::NW => Vector2::new(-1, -1),
            Self::SE => Vector2::new(1, 1),
            Self::SW => Vector2::new(-1, 1),
            _ => Vector2::new(0, 0),
        }
    }

    /// `Mask`: `1 << (int) m_dir`. A negative count or one of 32 or more is undefined; C++20
    /// defines `1 << 31` as `INT_MIN`.
    pub fn mask(&self) -> Direction45Result<i32> {
        u32::try_from(self.dir)
            .ok()
            .and_then(|shift| 1i32.checked_shl(shift))
            .ok_or(Direction45Error::Undefined)
    }

    /// `AllDirectionsMask`.
    pub const fn all_directions_mask() -> i32 {
        0xffff
    }

    /// `construct_`: the direction of `aVec` (already with `y` negated), rounded to the nearest
    /// multiple of 45 degrees clockwise from north.
    fn construct(&mut self, vec: Vector2I) {
        self.dir = Self::UNDEFINED;

        if vec.x == 0 && vec.y == 0 {
            return;
        }

        // 180.0 / M_PI is folded at compile time to the same correctly rounded constant.
        let mut mag = 360.0
            - (180.0 / core::f64::consts::PI
                * reference_libm::atan2(f64::from(vec.y), f64::from(vec.x)))
            + 90.0;

        if mag >= 360.0 {
            mag -= 360.0;
        }

        if mag < 0.0 {
            mag += 360.0;
        }

        // mag is in [0, 360], so the truncating conversion is in range.
        let mut dir = ((mag + 22.5) / 45.0) as i32;

        if dir >= Self::LAST {
            dir -= Self::LAST;
        }

        if dir < 0 {
            dir += Self::LAST;
        }

        self.dir = dir;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_follows_the_compass() {
        let dir = |x, y| Direction45::from_vector(Vector2::new(x, y), false).map(|d| d.dir());
        assert_eq!(dir(0, 0), Ok(Direction45::UNDEFINED));
        assert_eq!(dir(0, -10), Ok(Direction45::N));
        assert_eq!(dir(10, -10), Ok(Direction45::NE));
        assert_eq!(dir(10, 0), Ok(Direction45::E));
        assert_eq!(dir(10, 10), Ok(Direction45::SE));
        assert_eq!(dir(0, 10), Ok(Direction45::S));
        assert_eq!(dir(-10, 10), Ok(Direction45::SW));
        assert_eq!(dir(-10, 0), Ok(Direction45::W));
        assert_eq!(dir(-10, -10), Ok(Direction45::NW));
        assert_eq!(dir(0, i32::MIN), Err(Direction45Error::Undefined));
        assert_eq!(dir(i32::MIN, i32::MAX), Ok(Direction45::SW));
    }

    #[test]
    fn members_refuse_undefined_operations() {
        let d = Direction45::new;
        assert_eq!(d(-1).opposite(), Err(Direction45Error::Undefined));
        assert_eq!(d(8).opposite().map(|o| o.dir()), Ok(Direction45::UNDEFINED));
        assert_eq!(d(9).opposite(), Err(Direction45Error::Undefined));
        assert_eq!(d(31).mask(), Ok(i32::MIN));
        assert_eq!(d(32).mask(), Err(Direction45Error::Undefined));
        assert_eq!(d(-2).mask(), Err(Direction45Error::Undefined));
        assert_eq!(d(i32::MAX).right(), Err(Direction45Error::Undefined));
        assert_eq!(d(i32::MAX - 1).right().map(|r| r.dir()), Ok(i32::MAX % 8));
        assert_eq!(d(i32::MAX - 7).left(), Err(Direction45Error::Undefined));
        assert_eq!(d(-5).right().map(|r| r.dir()), Ok(-4));
        assert_eq!(d(i32::MIN).angle(&d(1)), Err(Direction45Error::Undefined));
        assert_eq!(
            d(i32::MIN + 1).angle(&d(1)),
            Err(Direction45Error::Undefined)
        );
        assert_eq!(d(i32::MIN + 2).angle(&d(1)), Ok(AngleType::AngStraight));
        assert_eq!(d(-1).angle(&d(i32::MIN)), Ok(AngleType::AngUndefined));
        assert!(!d(-1).is_diagonal());
        assert!(d(-7).right().is_ok_and(|r| !r.is_90deg()));
    }
}
