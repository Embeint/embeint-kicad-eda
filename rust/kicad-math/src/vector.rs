// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2010 Virtenio GmbH, Torsten Hueter, torsten.hueter <at> virtenio.de
// Copyright (C) 2012 SoftPLC Corporation, Dick Hollenbeck <dick@softplc.com>
// Copyright (C) 2013 CERN
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! Minimal co-port of `VECTOR2<T>` and `VECTOR3<T>` from `libs/kimath/include/math/vector2d.h`
//! and `vector3.h` at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! These definitions belong to `libs.kimath.include.math.application.ui.p002`; that unit
//! remains `todo`. Only the representation and the constructors that the matrix and wide
//! integer unit needs are provided, so that later ports extend these same types:
//!
//! | C++ definition (owned by ui.p002) | Rust |
//! | --- | --- |
//! | `VECTOR2<T>::VECTOR2()` (`x{}, y{}`) | `Default for Vector2<T>` |
//! | `VECTOR2<T>::VECTOR2( T, T )` | [`Vector2::new`] |
//! | `VECTOR2<T>::VECTOR2( const VECTOR2<T>& )` | `Clone`/`Copy` |
//! | `VECTOR3<T>::VECTOR3( T, T, T )` | [`Vector3::new`] |
//! | defaulted `VECTOR3<T>::VECTOR3()` with `x{}, y{}, z{}` | `Default for Vector3<T>` |
//!
//! The `BEZIER<T>` bodies of `libs.kimath.include.application.pure.p001` add the minimum
//! arithmetic and conversion they call, for the component types KiCad instantiates them with
//! (`int` and `double`):
//!
//! | C++ definition (owned by ui.p002), instantiation | Rust |
//! | --- | --- |
//! | free `operator-( const VECTOR2<T>&, const VECTOR2<U>& )`, `<int, int>` | [`Vector2::<i32>::checked_sub`] |
//! | the same, `<double, double>` | `Sub for Vector2<f64>` |
//! | free `operator*( const T&, const VECTOR2<U>& )`, `<double, int>` and `<double, double>` | `Mul<Vector2<i32>>`, `Mul<Vector2<f64>>` for `f64` |
//! | free `operator+( const VECTOR2<T>&, const VECTOR2<U>& )`, `<int, double>` and `<double, double>` | `Add<Vector2<f64>>` for `Vector2<i32>`, `Vector2<f64>` |
//! | casting constructor `VECTOR2<int>( const VECTOR2<double>& )` (floating source branch) | `TryFrom<Vector2<f64>> for Vector2<i32>` |
//!
//! `double` NaN results follow the oracle platform explicitly (private `fp` module). Every other
//! vector operation, including equality, the other overloads and instantiations, the other
//! casting branches and stream output, is still owned by ui.p002 and is deliberately absent
//! rather than guessed.

use core::ops::{Add, Mul, Sub};

use crate::fp;

/// `VECTOR2<T>`: a general 2D vector or point. Coordinates keep the C++ component type.
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector2<T> {
    pub x: T,
    pub y: T,
}

/// `VECTOR3<T>`: a general 3D vector.
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector3<T> {
    pub x: T,
    pub y: T,
    pub z: T,
}

impl<T> Vector2<T> {
    /// `VECTOR2( T x, T y )`: construct a vector with the given components.
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl<T> Vector3<T> {
    /// `VECTOR3( T x, T y, T z )`: construct a vector with the given components.
    pub const fn new(x: T, y: T, z: T) -> Self {
        Self { x, y, z }
    }
}

/// `VECTOR2D`
pub type Vector2D = Vector2<f64>;
/// `VECTOR2I` (`VECTOR2<int32_t>`)
pub type Vector2I = Vector2<i32>;
/// `VECTOR2L` (`VECTOR2<int64_t>`)
pub type Vector2L = Vector2<i64>;
/// `VECTOR3D`
pub type Vector3D = Vector3<f64>;
/// `VECTOR3I`
pub type Vector3I = Vector3<i32>;
/// `VECTOR3U`
pub type Vector3U = Vector3<u32>;

/// A co-ported `VECTOR2` operation whose C++ evaluation is undefined behaviour for these
/// arguments, so it has no reference result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorDomainError {
    /// `int` component arithmetic overflowed (signed overflow).
    IntegerOverflow,
    /// `static_cast<int>` of a NaN component; `std::clamp` passes NaN through unchanged.
    NanToInteger,
}

impl core::fmt::Display for VectorDomainError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::IntegerOverflow => "int component overflow",
            Self::NanToInteger => "NaN component converted to int",
        })
    }
}

impl Vector2<i32> {
    /// `operator-( const VECTOR2<int>&, const VECTOR2<int>& )` (vector2d.h:490-495): the
    /// result type is `VECTOR2<std::common_type_t<int, int>>`, so each component is an `int`
    /// subtraction, `x` then `y`. A component overflow is undefined in C++ and is refused.
    pub fn checked_sub(self, rhs: Self) -> Result<Self, VectorDomainError> {
        let x = self.x.checked_sub(rhs.x);
        let y = self.y.checked_sub(rhs.y);
        match (x, y) {
            (Some(x), Some(y)) => Ok(Self::new(x, y)),
            _ => Err(VectorDomainError::IntegerOverflow),
        }
    }
}

/// `operator-( const VECTOR2<double>&, const VECTOR2<double>& )` (vector2d.h:490-495).
impl Sub for Vector2<f64> {
    type Output = Vector2<f64>;

    fn sub(self, rhs: Self) -> Vector2<f64> {
        Vector2::new(fp::sub(self.x, rhs.x), fp::sub(self.y, rhs.y))
    }
}

/// `operator*( const double& aScalar, const VECTOR2<double>& )` (vector2d.h:541-545):
/// `VECTOR2<double>( aScalar * aVector.x, aScalar * aVector.y )`.
impl Mul<Vector2<f64>> for f64 {
    type Output = Vector2<f64>;

    fn mul(self, vector: Vector2<f64>) -> Vector2<f64> {
        Vector2::new(fp::mul(self, vector.x), fp::mul(self, vector.y))
    }
}

/// `operator*( const double& aScalar, const VECTOR2<int>& )` (vector2d.h:541-545): the
/// common type is `double`; each `int` component converts exactly to `double` before the
/// product.
impl Mul<Vector2<i32>> for f64 {
    type Output = Vector2<f64>;

    fn mul(self, vector: Vector2<i32>) -> Vector2<f64> {
        Vector2::new(
            fp::mul(self, f64::from(vector.x)),
            fp::mul(self, f64::from(vector.y)),
        )
    }
}

/// `operator+( const VECTOR2<double>&, const VECTOR2<double>& )` (vector2d.h:461-466).
impl Add<Vector2<f64>> for Vector2<f64> {
    type Output = Vector2<f64>;

    fn add(self, rhs: Vector2<f64>) -> Vector2<f64> {
        Vector2::new(fp::add(self.x, rhs.x), fp::add(self.y, rhs.y))
    }
}

/// `operator+( const VECTOR2<int>&, const VECTOR2<double>& )` (vector2d.h:461-466): the
/// common type is `double`; each `int` component converts exactly to `double` before the sum.
impl Add<Vector2<f64>> for Vector2<i32> {
    type Output = Vector2<f64>;

    fn add(self, rhs: Vector2<f64>) -> Vector2<f64> {
        Vector2::new(
            fp::add(f64::from(self.x), rhs.x),
            fp::add(f64::from(self.y), rhs.y),
        )
    }
}

/// The casting constructor `VECTOR2<int>( const VECTOR2<double>& )` (vector2d.h:84-99, the
/// `std::is_floating_point<CastingType>` branch): each component is
/// `static_cast<int>( std::clamp( v, double( INT_MIN ), double( INT_MAX ) ) )`, which
/// saturates infinities and out-of-range values and then truncates toward zero. It is also the
/// conversion C++ applies when a `VECTOR2<double>` initialises or is returned as a
/// `VECTOR2<int>`. `std::clamp` returns a NaN unchanged and converting it is undefined, so a NaN
/// component is refused.
impl TryFrom<Vector2<f64>> for Vector2<i32> {
    type Error = VectorDomainError;

    fn try_from(vector: Vector2<f64>) -> Result<Self, VectorDomainError> {
        Ok(Self::new(clamp_to_i32(vector.x)?, clamp_to_i32(vector.y)?))
    }
}

/// `static_cast<int>( std::clamp( aValue, minI, maxI ) )` with `minI`/`maxI` the exactly
/// representable `double( INT_MIN )` and `double( INT_MAX )`.
fn clamp_to_i32(value: f64) -> Result<i32, VectorDomainError> {
    const MIN: f64 = -2_147_483_648.0;
    const MAX: f64 = 2_147_483_647.0;
    // std::clamp( v, lo, hi ): ( v < lo ) ? lo : ( hi < v ) ? hi : v, spelled out so the
    // comparison order and the NaN pass-through are visibly the C++ ones.
    #[allow(clippy::manual_clamp)]
    let clamped = if value < MIN {
        MIN
    } else if MAX < value {
        MAX
    } else {
        value
    };
    if clamped.is_nan() {
        return Err(VectorDomainError::NanToInteger);
    }
    // In range after the clamp, so the conversion truncates toward zero exactly; Rust's
    // saturating `as` never saturates here.
    Ok(clamped as i32)
}
