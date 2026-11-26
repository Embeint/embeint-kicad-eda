// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2010 Virtenio GmbH, Torsten Hueter, torsten.hueter <at> virtenio.de
// Copyright (C) 2012 SoftPLC Corporation, Dick Hollenbeck <dick@softplc.com>
// Copyright (C) 2013 CERN
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! Canonical `VECTOR2<T>` and `VECTOR3<T>` port at KiCad pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7. See `TYPE_SUPPORT.md` for the
//! complete owned-body audit, supported instantiations and explicit deferrals.
//! Existing Bezier co-port operators retain their original contracts. New fallible
//! operations preserve checked signed arithmetic and report unavailable dependencies.

use core::ops::{Add, Mul, Sub};

use crate::fp;
use crate::math_util::{self, MathUtilError};

/// `VECTOR2<T>`: a general 2D vector or point. Coordinates keep the C++ component type.
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector2<T = i32> {
    pub x: T,
    pub y: T,
}

/// `VECTOR3<T>`: a general 3D vector.
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector3<T = i32> {
    pub x: T,
    pub y: T,
    pub z: T,
}

impl<T> Vector2<T> {
    /// `VECTOR2( T x, T y )`: construct a vector with the given components.
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }

    /// Memberwise assignment with the upstream mutable-self return contract.
    pub fn assign_from(&mut self, source: Self) -> &mut Self
    where
        T: Copy,
    {
        self.x = source.x;
        self.y = source.y;
        self
    }
}

impl<T> Vector3<T> {
    /// `VECTOR3( T x, T y, T z )`: construct a vector with the given components.
    pub const fn new(x: T, y: T, z: T) -> Self {
        Self { x, y, z }
    }

    /// The implicit C++ assignment also returns mutable self.
    pub fn assign_from(&mut self, source: Self) -> &mut Self
    where
        T: Copy,
    {
        self.x = source.x;
        self.y = source.y;
        self.z = source.z;
        self
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

/// A vector operation outside the defined C++ domain or still awaiting reference qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorDomainError {
    /// Signed component or extended arithmetic overflowed.
    IntegerOverflow,
    /// A NaN component converted to an integer; VECTOR2 clamping passes NaN through.
    NanToInteger,
    /// A floating-to-integral cast outside the actual C++ defined interval.
    CastOutOfRange,
    /// Integral division by zero.
    DivisionByZero,
    /// An unavailable helper or still unqualified floating implementation site.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

impl core::fmt::Display for VectorDomainError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::IntegerOverflow => "int component overflow",
            Self::NanToInteger => "NaN component converted to int",
            Self::CastOutOfRange => "floating component outside cast range",
            Self::DivisionByZero => "integer division by zero",
            Self::Unported { upstream, .. } => upstream,
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

const VECTOR_UNIT: &str = "libs.kimath.include.math.application.ui.p002";
type VResult<T> = Result<T, VectorDomainError>;

fn missing<T>(upstream: &'static str, unit: &'static str) -> VResult<T> {
    Err(VectorDomainError::Unported { upstream, unit })
}

/// Errors from the accepted util source bodies keep their meaning: a zero denominator is the
/// integral division the C++ would perform, and a deferred host call keeps its owner.
impl From<MathUtilError> for VectorDomainError {
    fn from(error: MathUtilError) -> Self {
        match error {
            MathUtilError::ZeroDenominator => Self::DivisionByZero,
            MathUtilError::IntermediateOverflow => Self::IntegerOverflow,
            MathUtilError::Unported { upstream, unit } => Self::Unported { upstream, unit },
        }
    }
}

/// Arithmetic for the pinned aliases. Extension to other C++ template types is unqualified.
/// The extended type of i32 is i64; i64, u32 and f64 retain their own type.
pub trait VectorComponent: Copy + Default + PartialOrd + core::fmt::Display {
    type Extended: VectorComponent;
    const MIN: Self;
    const MAX: Self;
    fn c_add(self, rhs: Self) -> VResult<Self>;
    fn c_sub(self, rhs: Self) -> VResult<Self>;
    fn c_mul(self, rhs: Self) -> VResult<Self>;
    fn c_div(self, rhs: Self) -> VResult<Self>;
    fn c_neg(self) -> VResult<Self>;
    fn c_abs(self) -> VResult<Self>;
    fn wide(self) -> Self::Extended;
    fn double(self) -> f64;
    fn from_double(value: f64) -> VResult<Self>;
    fn norm2(x: Self, y: Self) -> VResult<Self>;
    fn squared_norm2(x: Self, y: Self) -> VResult<Self::Extended>;
    fn div_double(self, rhs: f64) -> VResult<Self>;
    fn resize(x: Self, y: Self, length: Self) -> VResult<Vector2<Self>>;
    fn stream(self) -> VResult<String>;
    fn normalization_norm(x: Self, y: Self, z: Self) -> VResult<Self> {
        Vector3::new(x, y, z).euclidean_norm()
    }
}

fn overflow<T>(value: Option<T>) -> VResult<T> {
    value.ok_or(VectorDomainError::IntegerOverflow)
}

use crate::util::{round_i32, rounded_hypot};

// GCC's operand selection varies by compiled site when two quieted NaNs differ.
// The bound live oracle did establish the unambiguous cases: a single distinct
// quieted NaN value propagates, and invalid basic arithmetic without a NaN input
// produces x86's negative indefinite. This is a value rule, never a case lookup.
// UNPORTED(libs.kimath.include.math.application.ui.p002): distinct NaN operand sites
pub(crate) fn arithmetic_result(a: f64, b: f64, value: f64) -> VResult<f64> {
    if !value.is_nan() {
        return Ok(value);
    }
    const EXP: u64 = 0x7ff0_0000_0000_0000;
    const FRACTION: u64 = 0x000f_ffff_ffff_ffff;
    const QUIET: u64 = 0x0008_0000_0000_0000;
    let a = a.to_bits();
    let b = b.to_bits();
    let an = a & EXP == EXP && a & FRACTION != 0;
    let bn = b & EXP == EXP && b & FRACTION != 0;
    match (an, bn) {
        (false, false) => Ok(f64::from_bits(0xfff8_0000_0000_0000)),
        (true, false) => Ok(f64::from_bits(a | QUIET)),
        (false, true) => Ok(f64::from_bits(b | QUIET)),
        (true, true) if (a | QUIET) == (b | QUIET) => Ok(f64::from_bits(a | QUIET)),
        (true, true) => missing(
            "VECTOR floating arithmetic: distinct NaN operands",
            VECTOR_UNIT,
        ),
    }
}

pub(crate) fn vector_sqrt(value: f64) -> VResult<f64> {
    if value.is_nan() {
        // sqrt has one operand: its unique NaN is quieted, with sign/payload retained.
        return Ok(f64::from_bits(value.to_bits() | 0x0008_0000_0000_0000));
    }
    let result = value.sqrt();
    if result.is_nan() {
        // UNPORTED(libs.kimath.include.math.application.ui.p002): negative sqrt site qualification
        missing("VECTOR sqrt: negative operand site", VECTOR_UNIT)
    } else {
        Ok(result)
    }
}

macro_rules! signed_component {
    ($t:ty) => {
        impl VectorComponent for $t {
            type Extended = i64;
            const MIN: Self = <$t>::MIN;
            const MAX: Self = <$t>::MAX;
            fn c_add(self, rhs: Self) -> VResult<Self> {
                overflow(self.checked_add(rhs))
            }
            fn c_sub(self, rhs: Self) -> VResult<Self> {
                overflow(self.checked_sub(rhs))
            }
            fn c_mul(self, rhs: Self) -> VResult<Self> {
                overflow(self.checked_mul(rhs))
            }
            fn c_div(self, rhs: Self) -> VResult<Self> {
                if rhs == 0 {
                    return Err(VectorDomainError::DivisionByZero);
                }
                overflow(self.checked_div(rhs))
            }
            fn c_neg(self) -> VResult<Self> {
                overflow(self.checked_neg())
            }
            fn c_abs(self) -> VResult<Self> {
                overflow(self.checked_abs())
            }
            fn wide(self) -> i64 {
                self as i64
            }
            fn double(self) -> f64 {
                self as f64
            }
            fn from_double(v: f64) -> VResult<Self> {
                cast_signed(v, <$t>::MIN as f64, -(<$t>::MIN as f64)).map(|x| x as $t)
            }
            fn norm2(x: Self, y: Self) -> VResult<Self> {
                let n = rounded_hypot(x as i64, y as i64)?;
                Ok(n.min(<$t>::MAX as u64) as Self)
            }
            fn squared_norm2(x: Self, y: Self) -> VResult<i64> {
                let ax = x.unsigned_abs() as u128;
                let ay = y.unsigned_abs() as u128;
                // Equivalent to the narrowMax early return and uint64 saturation; no signed UB.
                Ok((ax * ax + ay * ay).min(i64::MAX as u128) as i64)
            }
            fn div_double(self, rhs: f64) -> VResult<Self> {
                // Upstream rounds to int even when T is int64_t.
                Ok(round_i32(self as f64 / rhs)? as Self)
            }
            fn resize(x: Self, y: Self, length: Self) -> VResult<Vector2<Self>> {
                if x == 0 && y == 0 {
                    return Ok(Vector2::new(0, 0));
                }
                let (nx, ny) = if VectorComponent::c_abs(x)? == VectorComponent::c_abs(y)? {
                    let n =
                        VectorComponent::c_abs(length)? as f64 * core::f64::consts::FRAC_1_SQRT_2;
                    (n, n)
                } else {
                    resize_general_lengths(x as i64, y as i64, length as i64)?
                };
                let nx = round_i32(nx)?;
                let ny = round_i32(ny)?;
                let nx = if x < 0 { nx.c_neg()? } else { nx } as Self;
                let ny = if y < 0 { ny.c_neg()? } else { ny } as Self;
                let sign = scalar_sign(length)? as Self;
                Vector2::new(nx, ny).try_mul_scalar(sign)
            }
            fn stream(self) -> VResult<String> {
                Ok(self.to_string())
            }
        }
    };
}
signed_component!(i32);
signed_component!(i64);

/// Resize's general branch for both signed aliases, whose `extended_type` is `int64_t`:
/// `std::sqrt( rescale( newLength_sq, x_sq, l_sq ) )` for x and then y, in source order.
/// The squares and their sum are checked because signed overflow is undefined in C++; the
/// accepted Linux `__int128` `rescale<int64_t>` body rounds the quotient half away from zero.
/// Its non-negative result converts exactly as the integral `std::sqrt` overload does.
fn resize_general_lengths(x: i64, y: i64, length: i64) -> VResult<(f64, f64)> {
    let xs = x.c_mul(x)?;
    let ys = y.c_mul(y)?;
    let ls = xs.c_add(ys)?;
    let ns = length.c_mul(length)?;
    let new_x = (math_util::rescale_i64_int128(ns, xs, ls)? as f64).sqrt();
    let new_y = (math_util::rescale_i64_int128(ns, ys, ls)? as f64).sqrt();
    Ok((new_x, new_y))
}

fn cast_signed(v: f64, min: f64, upper: f64) -> VResult<i64> {
    if v.is_nan() {
        return Err(VectorDomainError::NanToInteger);
    }
    let t = v.trunc();
    if t < min || t >= upper {
        return Err(VectorDomainError::CastOutOfRange);
    }
    Ok(t as i64)
}

impl VectorComponent for u32 {
    type Extended = Self;
    const MIN: Self = 0;
    const MAX: Self = u32::MAX;
    fn c_add(self, rhs: Self) -> VResult<Self> {
        Ok(self.wrapping_add(rhs))
    }
    fn c_sub(self, rhs: Self) -> VResult<Self> {
        Ok(self.wrapping_sub(rhs))
    }
    fn c_mul(self, rhs: Self) -> VResult<Self> {
        Ok(self.wrapping_mul(rhs))
    }
    fn c_div(self, rhs: Self) -> VResult<Self> {
        self.checked_div(rhs)
            .ok_or(VectorDomainError::DivisionByZero)
    }
    fn c_neg(self) -> VResult<Self> {
        Ok(self.wrapping_neg())
    }
    fn c_abs(self) -> VResult<Self> {
        Ok(self)
    }
    fn wide(self) -> Self {
        self
    }
    fn double(self) -> f64 {
        f64::from(self)
    }
    fn from_double(v: f64) -> VResult<Self> {
        if v.is_nan() {
            return Err(VectorDomainError::NanToInteger);
        }
        let t = v.trunc();
        if !(0.0..4_294_967_296.0).contains(&t) {
            return Err(VectorDomainError::CastOutOfRange);
        }
        Ok(t as Self)
    }
    fn norm2(_x: Self, _y: Self) -> VResult<Self> {
        // VECTOR2<unsigned> does not instantiate std::abs unambiguously; it is not an alias.
        missing(
            "VECTOR2<unsigned>::EuclideanNorm (unqualified instantiation)",
            VECTOR_UNIT,
        )
    }
    fn squared_norm2(x: Self, y: Self) -> VResult<Self> {
        x.c_mul(x)?.c_add(y.c_mul(y)?)
    }
    fn div_double(self, rhs: f64) -> VResult<Self> {
        Ok(round_i32(f64::from(self) / rhs)? as Self)
    }
    fn resize(_x: Self, _y: Self, _length: Self) -> VResult<Vector2<Self>> {
        missing(
            "VECTOR2<unsigned>::Resize (unqualified instantiation)",
            VECTOR_UNIT,
        )
    }
    fn stream(self) -> VResult<String> {
        Ok(self.to_string())
    }
}

impl VectorComponent for f64 {
    type Extended = Self;
    // C++ numeric_limits::min is the smallest positive NORMAL, not lowest().
    const MIN: Self = f64::MIN_POSITIVE;
    const MAX: Self = f64::MAX;
    fn c_add(self, rhs: Self) -> VResult<Self> {
        arithmetic_result(self, rhs, self + rhs)
    }
    fn c_sub(self, rhs: Self) -> VResult<Self> {
        arithmetic_result(self, rhs, self - rhs)
    }
    fn c_mul(self, rhs: Self) -> VResult<Self> {
        arithmetic_result(self, rhs, self * rhs)
    }
    fn c_div(self, rhs: Self) -> VResult<Self> {
        arithmetic_result(self, rhs, self / rhs)
    }
    fn c_neg(self) -> VResult<Self> {
        Ok(-self)
    }
    fn c_abs(self) -> VResult<Self> {
        Ok(self.abs())
    }
    fn wide(self) -> Self {
        self
    }
    fn double(self) -> Self {
        self
    }
    fn from_double(v: f64) -> VResult<Self> {
        Ok(v)
    }
    fn norm2(x: Self, y: Self) -> VResult<Self> {
        if x.abs() == y.abs() {
            return x.abs().c_mul(core::f64::consts::SQRT_2);
        }
        if x == 0.0 {
            return Ok(y.abs());
        }
        if y == 0.0 {
            return Ok(x.abs());
        }
        // static_cast<double>( std::hypot( x, y ) ). Finite operands use the shared glibc 2.39
        // finite-input backend (hypot.rs), wired by the arc_construction unit, which reaches this
        // body for every general chord; its oracle and the vector corpus check it exactly.
        if x.is_finite() && y.is_finite() {
            return Ok(crate::hypot::finite(x, y));
        }
        // UNPORTED(libs.kimath.include.math.application.ui.p002): std::hypot reference/libm parity
        // for infinite and NaN operands (the glibc special-value path and its NaN selection).
        missing("VECTOR2<double>::EuclideanNorm std::hypot", VECTOR_UNIT)
    }
    fn squared_norm2(x: Self, y: Self) -> VResult<Self> {
        x.c_mul(x)?.c_add(y.c_mul(y)?)
    }
    fn div_double(self, rhs: Self) -> VResult<Self> {
        self.c_div(rhs)
    }
    fn resize(x: Self, y: Self, length: Self) -> VResult<Vector2<Self>> {
        if x == 0.0 && y == 0.0 {
            return Ok(Vector2::new(0.0, 0.0));
        }
        let (nx, ny) = if x.abs() == y.abs() {
            let n = length.abs().c_mul(core::f64::consts::FRAC_1_SQRT_2)?;
            (n, n)
        } else {
            let xs = x.c_mul(x)?;
            let ys = y.c_mul(y)?;
            let ls = xs.c_add(ys)?;
            let ns = length.c_mul(length)?;
            let nx = rescale_f64(ns, xs, ls)?;
            let ny = rescale_f64(ns, ys, ls)?;
            (vector_sqrt(nx)?, vector_sqrt(ny)?)
        };
        let nx = if x < 0.0 { -nx } else { nx };
        let ny = if y < 0.0 { -ny } else { ny };
        let sign = f64::from(scalar_sign(length)?);
        Vector2::new(nx, ny).try_mul_scalar(sign)
    }
    fn normalization_norm(x: Self, y: Self, z: Self) -> VResult<Self> {
        // Normalize only observes whether a NaN norm is > 0, not its payload.
        // The check returns unchanged self, including all original signalling bits.
        Ok(((x * x + y * y) + z * z).sqrt())
    }
    fn stream(self) -> VResult<String> {
        // UNPORTED(libs.kimath.include.math.application.ui.p002): default ostream floating format
        missing("VECTOR stream<double> defaultfloat", VECTOR_UNIT)
    }
}

impl<T: PartialEq> PartialEq for Vector2<T> {
    fn eq(&self, rhs: &Self) -> bool {
        rhs.x == self.x && rhs.y == self.y
    }
    // Keep the independently spelled upstream inequality and its short-circuit order.
    #[allow(clippy::partialeq_ne_impl)]
    fn ne(&self, rhs: &Self) -> bool {
        rhs.x != self.x || rhs.y != self.y
    }
}
impl<T: PartialEq> PartialEq for Vector3<T> {
    fn eq(&self, rhs: &Self) -> bool {
        rhs.x == self.x && rhs.y == self.y && rhs.z == self.z
    }
    // Keep the independently spelled upstream inequality and its short-circuit order.
    #[allow(clippy::partialeq_ne_impl)]
    fn ne(&self, rhs: &Self) -> bool {
        rhs.x != self.x || rhs.y != self.y || rhs.z != self.z
    }
}
impl<T: PartialOrd> Vector2<T> {
    pub fn less(&self, rhs: &Self) -> bool {
        self.x < rhs.x || (self.x == rhs.x && self.y < rhs.y)
    }
    pub fn less_equal(&self, rhs: &Self) -> bool {
        self.x < rhs.x || (self.x == rhs.x && self.y <= rhs.y)
    }
    pub fn greater(&self, rhs: &Self) -> bool {
        self.x > rhs.x || (self.x == rhs.x && self.y > rhs.y)
    }
    pub fn greater_equal(&self, rhs: &Self) -> bool {
        self.x > rhs.x || (self.x == rhs.x && self.y >= rhs.y)
    }
    pub fn lexicographical_compare(&self, rhs: &Self) -> i32 {
        if self.x < rhs.x {
            -1
        } else if self.x > rhs.x {
            1
        } else if self.y < rhs.y {
            -1
        } else if self.y > rhs.y {
            1
        } else {
            0
        }
    }
    /// Returns the second reference on ties, preserving identity and all component bits.
    pub fn lexicographical_min<'a>(&'a self, rhs: &'a Self) -> &'a Self {
        if self.less(rhs) {
            self
        } else {
            rhs
        }
    }
    pub fn lexicographical_max<'a>(&'a self, rhs: &'a Self) -> &'a Self {
        if self.greater(rhs) {
            self
        } else {
            rhs
        }
    }
}
impl<T: VectorComponent> Vector2<T> {
    pub const ECOORD_MIN: T::Extended = <T::Extended as VectorComponent>::MIN;
    pub const ECOORD_MAX: T::Extended = <T::Extended as VectorComponent>::MAX;
    pub fn try_add(self, rhs: Self) -> VResult<Self> {
        Ok(Self::new(self.x.c_add(rhs.x)?, self.y.c_add(rhs.y)?))
    }
    pub fn try_sub(self, rhs: Self) -> VResult<Self> {
        Ok(Self::new(self.x.c_sub(rhs.x)?, self.y.c_sub(rhs.y)?))
    }
    pub fn try_mul_scalar(self, rhs: T) -> VResult<Self> {
        Ok(Self::new(self.x.c_mul(rhs)?, self.y.c_mul(rhs)?))
    }
    pub fn try_add_scalar(self, rhs: T) -> VResult<Self> {
        Ok(Self::new(self.x.c_add(rhs)?, self.y.c_add(rhs)?))
    }
    pub fn try_sub_scalar(self, rhs: T) -> VResult<Self> {
        Ok(Self::new(self.x.c_sub(rhs)?, self.y.c_sub(rhs)?))
    }
    pub fn try_neg(self) -> VResult<Self> {
        Ok(Self::new(self.x.c_neg()?, self.y.c_neg()?))
    }
    pub fn perpendicular(self) -> VResult<Self> {
        Ok(Self::new(self.y.c_neg()?, self.x))
    }
    pub fn dot(self, rhs: Self) -> VResult<T::Extended> {
        self.x
            .wide()
            .c_mul(rhs.x.wide())?
            .c_add(self.y.wide().c_mul(rhs.y.wide())?)
    }
    pub fn cross(self, rhs: Self) -> VResult<T::Extended> {
        self.x
            .wide()
            .c_mul(rhs.y.wide())?
            .c_sub(self.y.wide().c_mul(rhs.x.wide())?)
    }
    pub fn squared_distance(self, rhs: Self) -> VResult<T::Extended> {
        let dx = self.x.wide().c_sub(rhs.x.wide())?;
        let dy = self.y.wide().c_sub(rhs.y.wide())?;
        dx.c_mul(dx)?.c_add(dy.c_mul(dy)?)
    }
    pub fn distance(self, rhs: Vector2<T::Extended>) -> VResult<f64> {
        let diff = Vector2::new(
            rhs.x.c_sub(self.x.wide())?.double(),
            rhs.y.c_sub(self.y.wide())?.double(),
        );
        diff.euclidean_norm()
    }
    pub fn squared_euclidean_norm(self) -> VResult<T::Extended> {
        T::squared_norm2(self.x, self.y)
    }
    pub fn euclidean_norm(self) -> VResult<T> {
        T::norm2(self.x, self.y)
    }
    pub fn resize(self, length: T) -> VResult<Self> {
        T::resize(self.x, self.y, length)
    }
    pub fn divide(self, factor: f64) -> VResult<Self> {
        Ok(Self::new(
            self.x.div_double(factor)?,
            self.y.div_double(factor)?,
        ))
    }
    pub fn format(self) -> VResult<String> {
        Ok(format!("( xy {} {} )", self.x.stream()?, self.y.stream()?))
    }
    pub fn stream(self) -> VResult<String> {
        Ok(format!("[ {} | {} ]", self.x.stream()?, self.y.stream()?))
    }
    // Compound operations mutate in source order. If the second component fails,
    // the first write is retained; no transaction is invented around a C++ body.
    pub fn add_assign(&mut self, rhs: Self) -> VResult<&mut Self> {
        self.x = self.x.c_add(rhs.x)?;
        self.y = self.y.c_add(rhs.y)?;
        Ok(self)
    }
    pub fn sub_assign(&mut self, rhs: Self) -> VResult<&mut Self> {
        self.x = self.x.c_sub(rhs.x)?;
        self.y = self.y.c_sub(rhs.y)?;
        Ok(self)
    }
    pub fn mul_assign(&mut self, rhs: Self) -> VResult<&mut Self> {
        self.x = self.x.c_mul(rhs.x)?;
        self.y = self.y.c_mul(rhs.y)?;
        Ok(self)
    }
    pub fn mul_assign_scalar(&mut self, rhs: T) -> VResult<&mut Self> {
        self.mul_assign(Self::new(rhs, rhs))
    }
    pub fn add_assign_scalar(&mut self, rhs: T) -> VResult<&mut Self> {
        self.add_assign(Self::new(rhs, rhs))
    }
    pub fn sub_assign_scalar(&mut self, rhs: T) -> VResult<&mut Self> {
        self.sub_assign(Self::new(rhs, rhs))
    }
}
impl Vector2<f64> {
    /// Both header declarations share one body; the omitted epsilon is f64::EPSILON.
    pub fn equals(self, rhs: Self, epsilon: Option<f64>) -> VResult<bool> {
        let e = epsilon.unwrap_or(f64::EPSILON);
        if !scalar_equals(self.x, rhs.x, e)? {
            return Ok(false);
        }
        scalar_equals(self.y, rhs.y, e)
    }
    pub fn ki_round(self) -> VResult<Vector2<i32>> {
        Ok(Vector2::new(round_i32(self.x)?, round_i32(self.y)?))
    }
}
fn scalar_equals(a: f64, b: f64, e: f64) -> VResult<bool> {
    Ok(crate::util::equals_f64(a, b, Some(e)))
}
pub fn ki_round_xy(x: f64, y: f64) -> VResult<Vector2<i32>> {
    Vector2::new(x, y).ki_round()
}

impl<T: VectorComponent> Vector3<T> {
    pub const ECOORD_MIN: T::Extended = <T::Extended as VectorComponent>::MIN;
    pub const ECOORD_MAX: T::Extended = <T::Extended as VectorComponent>::MAX;
    pub fn cross(self, rhs: Self) -> VResult<Self> {
        // Products and subtraction are in T, NOT extended_type, even if cancellation fits.
        Ok(Self::new(
            self.y.c_mul(rhs.z)?.c_sub(self.z.c_mul(rhs.y)?)?,
            self.z.c_mul(rhs.x)?.c_sub(self.x.c_mul(rhs.z)?)?,
            self.x.c_mul(rhs.y)?.c_sub(self.y.c_mul(rhs.x)?)?,
        ))
    }
    pub fn dot(self, rhs: Self) -> VResult<T::Extended> {
        self.x
            .wide()
            .c_mul(rhs.x.wide())?
            .c_add(self.y.wide().c_mul(rhs.y.wide())?)?
            .c_add(self.z.wide().c_mul(rhs.z.wide())?)
    }
    pub fn euclidean_norm(self) -> VResult<T> {
        let sum = self
            .x
            .wide()
            .c_mul(self.x.wide())?
            .c_add(self.y.wide().c_mul(self.y.wide())?)?
            .c_add(self.z.wide().c_mul(self.z.wide())?)?;
        T::from_double(vector_sqrt(sum.double())?)
    }
    /// wxCHECK_MSG returns unchanged self by value for norm <= 0 or unordered.
    pub fn normalize(&mut self) -> VResult<Self> {
        let norm = T::normalization_norm(self.x, self.y, self.z)?;
        if norm > T::default() {
            self.x = self.x.c_div(norm)?;
            self.y = self.y.c_div(norm)?;
            self.z = self.z.c_div(norm)?;
        }
        Ok(*self)
    }
    pub fn set_all(&mut self, value: T) -> Self {
        self.x = value;
        self.y = value;
        self.z = value;
        *self
    }
    pub fn mul_assign_scalar(&mut self, value: T) -> VResult<&mut Self> {
        self.x = self.x.c_mul(value)?;
        self.y = self.y.c_mul(value)?;
        self.z = self.z.c_mul(value)?;
        Ok(self)
    }
    pub fn div_assign_scalar(&mut self, value: T) -> VResult<&mut Self> {
        self.x = self.x.c_div(value)?;
        self.y = self.y.c_div(value)?;
        self.z = self.z.c_div(value)?;
        Ok(self)
    }
    pub fn stream(self) -> VResult<String> {
        Ok(format!(
            "[ {} | {} | {} ]",
            self.x.stream()?,
            self.y.stream()?,
            self.z.stream()?
        ))
    }
}

/// Conversion semantics of the VECTOR2 casting constructor/operator().
pub trait Vector2CastFrom<T>: Sized {
    fn cast2(value: T) -> VResult<Self>;
}
/// Conversion semantics of VECTOR3's unclamped converting constructor.
pub trait Vector3CastFrom<T>: Sized {
    fn cast3(value: T) -> VResult<Self>;
}
impl<T: Copy> Vector2<T> {
    pub fn try_cast<U: Vector2CastFrom<T>>(self) -> VResult<Vector2<U>> {
        Ok(Vector2::new(U::cast2(self.x)?, U::cast2(self.y)?))
    }
}
impl<T: Copy> Vector3<T> {
    pub fn try_cast<U: Vector3CastFrom<T>>(self) -> VResult<Vector3<U>> {
        Ok(Vector3::new(
            U::cast3(self.x)?,
            U::cast3(self.y)?,
            U::cast3(self.z)?,
        ))
    }
}
macro_rules! identity_cast {
    ($t:ty) => {
        impl Vector2CastFrom<$t> for $t {
            fn cast2(v: $t) -> VResult<Self> {
                Ok(v)
            }
        }
        impl Vector3CastFrom<$t> for $t {
            fn cast3(v: $t) -> VResult<Self> {
                Ok(v)
            }
        }
    };
}
identity_cast!(i32);
identity_cast!(i64);
identity_cast!(u32);
identity_cast!(f64);
macro_rules! integer_cast {
    ($src:ty, $dst:ty) => {
        impl Vector2CastFrom<$src> for $dst {
            fn cast2(v: $src) -> VResult<Self> {
                Ok((v as i64).clamp(<$dst>::MIN as i64, <$dst>::MAX as i64) as Self)
            }
        }
        impl Vector3CastFrom<$src> for $dst {
            fn cast3(v: $src) -> VResult<Self> {
                Ok(v as Self)
            }
        }
    };
}
integer_cast!(i32, i64);
integer_cast!(i64, i32);
integer_cast!(i32, u32);
integer_cast!(u32, i32);
integer_cast!(i64, u32);
integer_cast!(u32, i64);
macro_rules! float_cast {
    ($t:ty) => {
        impl Vector2CastFrom<$t> for f64 {
            fn cast2(v: $t) -> VResult<Self> {
                Ok(v as Self)
            }
        }
        impl Vector3CastFrom<$t> for f64 {
            fn cast3(v: $t) -> VResult<Self> {
                Ok(v as Self)
            }
        }
        impl Vector2CastFrom<f64> for $t {
            fn cast2(v: f64) -> VResult<Self> {
                let clamped = if v < <$t>::MIN as f64 {
                    <$t>::MIN as f64
                } else if (<$t>::MAX as f64) < v {
                    <$t>::MAX as f64
                } else {
                    v
                };
                // i64::MAX rounds to 2^63: clamping can still leave an undefined cast.
                <$t as VectorComponent>::from_double(clamped)
            }
        }
        impl Vector3CastFrom<f64> for $t {
            fn cast3(v: f64) -> VResult<Self> {
                <$t as VectorComponent>::from_double(v)
            }
        }
    };
}
float_cast!(i32);
float_cast!(i64);
float_cast!(u32);

/// C++ std::common_type for the measured arithmetic component types.
pub trait VectorCommon<Rhs>: VectorComponent {
    type Common: VectorComponent;
    fn promote_left(self) -> Self::Common;
    fn promote_right(value: Rhs) -> Self::Common;
}
macro_rules! common {
    ($a:ty, $b:ty, $c:ty) => {
        impl VectorCommon<$b> for $a {
            type Common = $c;
            fn promote_left(self) -> $c {
                self as $c
            }
            fn promote_right(v: $b) -> $c {
                v as $c
            }
        }
    };
}
common!(i32, i32, i32);
common!(i32, i64, i64);
common!(i32, f64, f64);
common!(i32, u32, u32);
common!(i64, i32, i64);
common!(i64, i64, i64);
common!(i64, f64, f64);
common!(i64, u32, i64);
common!(f64, i32, f64);
common!(f64, i64, f64);
common!(f64, f64, f64);
common!(f64, u32, f64);
common!(u32, i32, u32);
common!(u32, i64, i64);
common!(u32, f64, f64);
common!(u32, u32, u32);

pub fn vector2_add<T: VectorCommon<U>, U: Copy>(
    a: Vector2<T>,
    b: Vector2<U>,
) -> VResult<Vector2<T::Common>> {
    Vector2::new(a.x.promote_left(), a.y.promote_left())
        .try_add(Vector2::new(T::promote_right(b.x), T::promote_right(b.y)))
}
pub fn vector2_sub<T: VectorCommon<U>, U: Copy>(
    a: Vector2<T>,
    b: Vector2<U>,
) -> VResult<Vector2<T::Common>> {
    Vector2::new(a.x.promote_left(), a.y.promote_left())
        .try_sub(Vector2::new(T::promote_right(b.x), T::promote_right(b.y)))
}
pub fn vector2_dot<T: VectorCommon<U>, U: Copy>(
    a: Vector2<T>,
    b: Vector2<U>,
) -> VResult<<T::Common as VectorComponent>::Extended> {
    Vector2::new(a.x.promote_left(), a.y.promote_left())
        .dot(Vector2::new(T::promote_right(b.x), T::promote_right(b.y)))
}
pub fn vector2_mul<T: VectorCommon<U>, U: Copy>(
    a: Vector2<T>,
    b: U,
) -> VResult<Vector2<T::Common>> {
    Vector2::new(a.x.promote_left(), a.y.promote_left()).try_mul_scalar(T::promote_right(b))
}
pub fn scalar_mul_vector2<T: VectorCommon<U>, U: Copy>(
    a: T,
    b: Vector2<U>,
) -> VResult<Vector2<T::Common>> {
    let a = a.promote_left();
    Ok(Vector2::new(
        a.c_mul(T::promote_right(b.x))?,
        a.c_mul(T::promote_right(b.y))?,
    ))
}
macro_rules! scalar_float {
    ($t:ty) => {
        impl Vector2<$t> {
            pub fn add_float(self, scalar: f64) -> VResult<Self> {
                Ok(Self::new(
                    round_i32(self.x as f64 + scalar)? as $t,
                    round_i32(self.y as f64 + scalar)? as $t,
                ))
            }
            pub fn sub_float(self, scalar: f64) -> VResult<Self> {
                Ok(Self::new(
                    round_i32(self.x as f64 - scalar)? as $t,
                    round_i32(self.y as f64 - scalar)? as $t,
                ))
            }
        }
    };
}
scalar_float!(i32);
scalar_float!(i64);

/// C++ const-T-reference scalar binding, including references into the destination vector.
#[derive(Clone, Copy, Debug)]
pub enum Vector2Scalar<T> {
    Value(T),
    X,
    Y,
}
impl<T: Copy> Vector2Scalar<T> {
    fn read(self, vector: &Vector2<T>) -> T {
        match self {
            Self::Value(v) => v,
            Self::X => vector.x,
            Self::Y => vector.y,
        }
    }
}
impl<T: VectorComponent> Vector2<T> {
    pub fn mul_assign_operand(&mut self, rhs: Vector2Scalar<T>) -> VResult<&mut Self> {
        self.x = self.x.c_mul(rhs.read(self))?;
        self.y = self.y.c_mul(rhs.read(self))?;
        Ok(self)
    }
    pub fn add_assign_operand(&mut self, rhs: Vector2Scalar<T>) -> VResult<&mut Self> {
        self.x = self.x.c_add(rhs.read(self))?;
        self.y = self.y.c_add(rhs.read(self))?;
        Ok(self)
    }
    pub fn sub_assign_operand(&mut self, rhs: Vector2Scalar<T>) -> VResult<&mut Self> {
        self.x = self.x.c_sub(rhs.read(self))?;
        self.y = self.y.c_sub(rhs.read(self))?;
        Ok(self)
    }
}

fn rescale_f64(n: f64, v: f64, d: f64) -> VResult<f64> {
    crate::util::rescale(n, v, d)
}
fn scalar_sign<T: VectorComponent>(value: T) -> VResult<i32> {
    Ok(crate::util::sign(value))
}

impl<T: Eq> Eq for Vector2<T> {}
impl<T: Eq> Eq for Vector3<T> {}
impl<T: PartialOrd> PartialOrd for Vector2<T> {
    fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
        match self.x.partial_cmp(&rhs.x)? {
            core::cmp::Ordering::Equal => self.y.partial_cmp(&rhs.y),
            order => Some(order),
        }
    }
    fn lt(&self, rhs: &Self) -> bool {
        self.less(rhs)
    }
    fn le(&self, rhs: &Self) -> bool {
        self.less_equal(rhs)
    }
    fn gt(&self, rhs: &Self) -> bool {
        self.greater(rhs)
    }
    fn ge(&self, rhs: &Self) -> bool {
        self.greater_equal(rhs)
    }
}
macro_rules! integer_order {
    ($t:ty) => {
        impl Ord for Vector2<$t> {
            fn cmp(&self, rhs: &Self) -> core::cmp::Ordering {
                self.x.cmp(&rhs.x).then_with(|| self.y.cmp(&rhs.y))
            }
        }
    };
}
integer_order!(i32);
integer_order!(i64);

#[cfg(test)]
mod tests {
    use super::*;

    /// Resize's general continuation now runs through the accepted `rescale<int64_t>`; KiROUND
    /// (util.h) still defers afterwards, so its intermediate lengths are checked here.
    #[test]
    fn resize_general_lengths_use_the_accepted_rescale() {
        // rescale( 100, 9, 25 ) = ( 900 + 12 ) / 25 = 36 and ( 1600 + 12 ) / 25 = 64.
        assert_eq!(resize_general_lengths(3, 4, 10), Ok((6.0, 8.0)));
        // Signs only enter through the squares; rounding is half away from zero: 1 * 1 / 2.
        assert_eq!(resize_general_lengths(-1, 1, -1).map(|v| v.0), Ok(1.0));
        assert_eq!(
            resize_general_lengths(-3, 4, -10),
            resize_general_lengths(3, -4, 10)
        );
        // A product beyond int64 needs the __int128 intermediate: rescale( s, s, s + 1 ) with
        // s = 3037000499^2 is ( s * s + ( s + 1 ) / 2 ) / ( s + 1 ) = s - 1 exactly.
        let s = 3_037_000_499_i64 * 3_037_000_499;
        assert_eq!(
            resize_general_lengths(3_037_000_499, 1, 3_037_000_499),
            Ok((((s - 1) as f64).sqrt(), 1.0))
        );
        // int64 squares overflow before rescale; that undefined C++ path is refused.
        assert_eq!(
            resize_general_lengths(3_037_000_500, 1, 1),
            Err(VectorDomainError::IntegerOverflow)
        );
    }
}
