// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad math/util.h, copyright Michael Niedermayer, CERN and KiCad Developers.
//! Header utilities at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
use crate::math_util;
use crate::vector::{VectorComponent, VectorDomainError};
pub const UNIT: &str = "libs.kimath.include.math.application.ui.p001";
pub type Result<T> = core::result::Result<T, VectorDomainError>;

/// C++ std::min/max/clamp keep the first operand on equality and unordered comparisons.
pub fn min<T: Copy + PartialOrd>(a: T, b: T) -> T {
    if b < a {
        b
    } else {
        a
    }
}
pub fn max<T: Copy + PartialOrd>(a: T, b: T) -> T {
    if a < b {
        b
    } else {
        a
    }
}
pub fn clamp<T: Copy + PartialOrd>(v: T, lo: T, hi: T) -> Result<T> {
    if hi < lo {
        return Err(VectorDomainError::CastOutOfRange);
    }
    Ok(min(max(v, lo), hi))
}
pub fn sign<T: Copy + Default + PartialOrd>(v: T) -> i32 {
    i32::from(T::default() < v) - i32::from(v < T::default())
}
/// Integer specializations are the accepted util.cpp implementations, never copies.
pub trait Rescale: Sized {
    fn rescale(n: Self, v: Self, d: Self) -> Result<Self>;
}
pub fn rescale<T: Rescale>(n: T, v: T, d: T) -> Result<T> {
    T::rescale(n, v, d)
}
impl Rescale for i32 {
    fn rescale(n: Self, v: Self, d: Self) -> Result<Self> {
        Ok(math_util::rescale_i32(n, v, d)?)
    }
}
impl Rescale for i64 {
    fn rescale(n: Self, v: Self, d: Self) -> Result<Self> {
        Ok(math_util::rescale_i64_int128(n, v, d)?)
    }
}
impl Rescale for f64 {
    fn rescale(n: Self, v: Self, d: Self) -> Result<Self> {
        n.c_mul(v)?.c_div(d)
    }
}
impl Rescale for f32 {
    fn rescale(n: Self, v: Self, d: Self) -> Result<Self> {
        let product = fp32_binary(n, v, n * v)?;
        fp32_binary(product, d, product / d)
    }
}

// Transport f32 NaN payloads to the canonical vector arithmetic rule, then back.
// Finite arithmetic still rounds after each f32 operation, exactly as the template does.
fn fp32_binary(a: f32, b: f32, result: f32) -> Result<f32> {
    let widen = |v: f32| {
        if v.is_nan() {
            let bits = v.to_bits();
            f64::from_bits(
                (u64::from(bits & 0x80000000) << 32)
                    | 0x7ff0000000000000
                    | (u64::from(bits & 0x007fffff) << 29),
            )
        } else {
            f64::from(v)
        }
    };
    let selected = crate::vector::arithmetic_result(widen(a), widen(b), widen(result))?;
    if !result.is_nan() {
        return Ok(result);
    }
    let bits = selected.to_bits();
    Ok(f32::from_bits(
        ((bits >> 32) as u32 & 0x80000000) | 0x7f800000 | ((bits >> 29) as u32 & 0x007fffff),
    ))
}

// Small integer operands undergo the C++ integral promotions before multiplication.
macro_rules! promoted_rescale {($($t:ty),*)=>{$(
    impl Rescale for $t {fn rescale(n:Self,v:Self,d:Self)->Result<Self>{
        let product=i32::from(n).checked_mul(i32::from(v)).ok_or(VectorDomainError::IntegerOverflow)?;
        if d==0{return Err(VectorDomainError::DivisionByZero);}
        Ok(product.checked_div(i32::from(d)).ok_or(VectorDomainError::IntegerOverflow)? as Self)
    }}
)*};}
promoted_rescale!(i8, u8, i16, u16);
macro_rules! unsigned_rescale {($($t:ty),*)=>{$(
    impl Rescale for $t {fn rescale(n:Self,v:Self,d:Self)->Result<Self>{
        n.wrapping_mul(v).checked_div(d).ok_or(VectorDomainError::DivisionByZero)
    }}
)*};}
unsigned_rescale!(u32, u64);

pub trait RoundInteger: Copy {
    const LOW: i128;
    const HIGH: i128;
    const TYPE_NAME: &'static str;
    fn from_bounded(v: i128) -> Self;
}
macro_rules! integer { ($($t:ty => $name:literal),*) => {$ (
    impl RoundInteger for $t {
        const LOW:i128=<$t>::MIN as i128;
        const HIGH:i128=<$t>::MAX as i128;
        const TYPE_NAME:&'static str=$name;
        fn from_bounded(v:i128)->Self { v as Self }
    }
)*}; }
integer!(i8=>"a",u8=>"h",i16=>"s",u16=>"t",i32=>"i",u32=>"j",i64=>"l");

pub trait RoundSource: Copy {
    fn round_to<R: RoundInteger>(self, quiet: bool) -> Result<R>;
}
fn overflow<R: RoundInteger>(value: f64, result: i128, quiet: bool) -> Result<R> {
    if !quiet {
        // UNPORTED(libs.kimath.src.math.util.application.ui.p001): kimathLogOverflow
        math_util::kimath_log_overflow(value, R::TYPE_NAME)?;
    }
    Ok(R::from_bounded(result))
}
/// KiROUND's integral branch compares integers without conversion through double.
macro_rules! source_integer { ($($t:ty),*) => {$ (
    impl RoundSource for $t { fn round_to<R:RoundInteger>(self,quiet:bool)->Result<R> {
        let v=self as i128;
        if v>R::HIGH { return overflow::<R>(self as f64,R::HIGH,quiet); }
        if v<R::LOW { return overflow::<R>(self as f64,R::LOW,quiet); }
        Ok(R::from_bounded(v))
    }}
)*}; }
source_integer!(i8, u8, i16, u16, i32, u32, i64, u64);
impl RoundSource for f64 {
    fn round_to<R: RoundInteger>(self, quiet: bool) -> Result<R> {
        if self.is_nan() {
            return overflow::<R>(self, 0, quiet);
        }
        if self >= R::HIGH as f64 + 0.5 {
            return overflow::<R>(self, R::HIGH, quiet);
        }
        if self - R::LOW as f64 <= -0.5 {
            return overflow::<R>(self, R::LOW, quiet);
        }
        let rounded = self.round() as i128;
        let clamped = rounded.clamp(R::LOW, R::HIGH);
        if clamped != rounded {
            return overflow::<R>(self, clamped, quiet);
        }
        Ok(R::from_bounded(clamped))
    }
}
impl RoundSource for f32 {
    fn round_to<R: RoundInteger>(self, quiet: bool) -> Result<R> {
        if self.is_nan() {
            return overflow::<R>(f64::from(self), 0, quiet);
        }
        // The +0.5 and comparison promote the converted float bound to double in C++.
        if f64::from(self) >= f64::from(R::HIGH as f32) + 0.5 {
            return overflow::<R>(f64::from(self), R::HIGH, quiet);
        }
        if f64::from(self - R::LOW as f32) <= -0.5 {
            return overflow::<R>(f64::from(self), R::LOW, quiet);
        }
        let rounded = f64::from(self).round() as i128;
        let clamped = rounded.clamp(R::LOW, R::HIGH);
        if clamped != rounded {
            return overflow::<R>(f64::from(self), clamped, quiet);
        }
        Ok(R::from_bounded(clamped))
    }
}
pub fn ki_round<S: RoundSource, R: RoundInteger>(value: S, quiet: bool) -> Result<R> {
    value.round_to::<R>(quiet)
}
pub fn round_i32(value: f64) -> Result<i32> {
    ki_round(value, false)
}

/// The special `long long -> int` overload. Linux `int64_t` is *long*, not long long;
/// BOX2's explicitly instantiated generic casts use `BoxCoord::narrow` instead.
pub fn checked_cast_long_long_to_int(value: i64) -> Result<i32> {
    if value > i64::from(i32::MAX) {
        return overflow::<i32>(value as f64, i128::from(i32::MAX), false);
    }
    if value < i64::from(i32::MIN) {
        return overflow::<i32>(value as f64, i128::from(i32::MIN), false);
    }
    Ok(value as i32)
}
/// Generic integral KiCheckedCast is ordinary C++20 modulo narrowing, without a clamp.
pub fn checked_cast_integral<S: Into<i128>, R: RoundInteger>(v: S) -> R {
    R::from_bounded(v.into())
}

pub fn equals_f64(a: f64, b: f64, epsilon: Option<f64>) -> bool {
    let e = epsilon.unwrap_or(f64::EPSILON);
    let diff = (a - b).abs();
    if diff < e {
        return true;
    }
    let a = a.abs();
    let b = b.abs();
    let largest = if a > b { a } else { b };
    diff <= largest * e
}
pub fn equals_f32(a: f32, b: f32, epsilon: Option<f32>) -> bool {
    let e = epsilon.unwrap_or(f32::EPSILON);
    let diff = (a - b).abs();
    if diff < e {
        return true;
    }
    let a = a.abs();
    let b = b.abs();
    let largest = if a > b { a } else { b };
    diff <= largest * e
}

pub trait SqrtInteger: Copy {
    const MAX: i128;
    fn integer(self) -> i128;
    fn from_integer(v: i128) -> Self;
}
macro_rules! sqrt_integer { ($($t:ty),*) => {$ (
    impl SqrtInteger for $t { const MAX:i128=<$t>::MAX as i128;
        fn integer(self)->i128 { self as i128 } fn from_integer(v:i128)->Self {v as Self}
    }
)*}; }
sqrt_integer!(i8, u8, i16, u16, i32, u32, i64, u64);
fn bounded<T: SqrtInteger>(v: Option<i128>) -> Result<i128> {
    v.filter(|v| *v >= -T::MAX - 1 && *v <= T::MAX)
        .ok_or(VectorDomainError::IntegerOverflow)
}
pub fn ct_sqrt_helper<T: SqrtInteger>(x: T, lo: T, hi: T) -> Result<T> {
    let x = x.integer();
    let mut lo = lo.integer();
    let mut hi = hi.integer();
    // For a defined convergent invocation, each iteration halves the interval.
    for _ in 0..128 {
        if lo == hi {
            return Ok(T::from_integer(lo));
        }
        let mid = bounded::<T>(bounded::<T>(lo.checked_add(hi))?.checked_add(1))? / 2;
        if mid == 0 {
            return Err(VectorDomainError::DivisionByZero);
        }
        if x / mid < mid {
            hi = bounded::<T>(mid.checked_sub(1))?;
        } else {
            lo = mid;
        }
    }
    Err(VectorDomainError::CastOutOfRange)
}
pub fn ct_sqrt<T: SqrtInteger>(x: T) -> Result<T> {
    ct_sqrt_helper(
        x,
        T::from_integer(0),
        T::from_integer(bounded::<T>((x.integer() / 2).checked_add(1))?),
    )
}
pub fn isqrt<T: SqrtInteger>(x: T) -> Result<T> {
    let sqrt_max = ct_sqrt(T::from_integer(T::MAX))?.integer();
    let x = x.integer();
    if x < 0 {
        return Ok(T::from_integer(sqrt_max));
    }
    let mut r = (x as f64).sqrt() as i128;
    while r < sqrt_max && r * r < x {
        r += 1;
    }
    while r > sqrt_max || r * r > x {
        r -= 1;
    }
    Ok(T::from_integer(r))
}
pub const fn unsigned_abs(x: i64) -> u64 {
    x.unsigned_abs()
}
pub fn rounded_hypot(x: i64, y: i64) -> Result<u64> {
    let ax = unsigned_abs(x);
    let ay = unsigned_abs(y);
    let narrow_max = ct_sqrt(i64::MAX as u64)?;
    if ax > narrow_max || ay > narrow_max {
        return Ok(math_util::rounded_hypot_wide(ax, ay));
    }
    let n = ax * ax + ay * ay;
    let f = isqrt(n)?;
    Ok(f + u64::from(n - f * f > f))
}

/// Generic KiCheckedCast branch: implicit numeric conversion. C++'s long long
/// specialization is exposed separately because Rust i64 also represents Linux long.
pub trait CheckedCast<R> {
    fn checked_cast(self) -> Result<R>;
}
pub fn ki_checked_cast<S: CheckedCast<R>, R>(value: S) -> Result<R> {
    value.checked_cast()
}
macro_rules! cast_integral {
    ($s:ty; $($r:ty),*)=>{$(impl CheckedCast<$r> for $s{fn checked_cast(self)->Result<$r>{Ok(self as $r)}})*};
}
macro_rules! cast_sources {($($s:ty),*)=>{$(cast_integral!($s;i8,u8,i16,u16,i32,u32,i64,u64,f32,f64);)*};}
cast_sources!(i8, u8, i16, u16, i32, u32, i64, u64);
macro_rules! cast_float_integer {
    ($s:ty; $($r:ty),*)=>{$(impl CheckedCast<$r> for $s {fn checked_cast(self)->Result<$r>{
        let v=f64::from(self).trunc();
        if v.is_nan(){return Err(VectorDomainError::NanToInteger);}
        // Exact power-of-two upper bound; the rounded double(MAX) is not safe for i64/u64.
        if v < <$r>::MIN as f64 || v >= (<$r>::MAX as u128 + 1) as f64 {
            return Err(VectorDomainError::CastOutOfRange);
        }
        Ok(v as $r)
    }})*};
}
cast_float_integer!(f32;i8,u8,i16,u16,i32,u32,i64,u64);
cast_float_integer!(f64;i8,u8,i16,u16,i32,u32,i64,u64);
impl CheckedCast<f64> for f64 {
    fn checked_cast(self) -> Result<f64> {
        Ok(self)
    }
}
impl CheckedCast<f32> for f32 {
    fn checked_cast(self) -> Result<f32> {
        Ok(self)
    }
}
impl CheckedCast<f64> for f32 {
    fn checked_cast(self) -> Result<f64> {
        Ok(f64::from(self))
    }
}
impl CheckedCast<f32> for f64 {
    fn checked_cast(self) -> Result<f32> {
        Ok(self as f32)
    }
}
