// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad eda_angle.h, copyright The KiCad Developers.
//! Angle magnitudes and normalized orientations at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
use crate::{fp, glibc_trig, StreamError, StreamInsert, StreamState, Vector2D};
use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

pub const UNIT: &str = "libs.kimath.include.geometry.application.ui.p001";
pub const UTIL_HEADER: &str = "libs.kimath.include.math.application.ui.p001";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryError {
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
    IntegerOverflow,
    ResourceLimit,
}
pub type GeometryResult<T> = Result<T, GeometryError>;
pub(crate) fn missing<T>(upstream: &'static str, unit: &'static str) -> GeometryResult<T> {
    Err(GeometryError::Unported { upstream, unit })
}
// Division has the same single-NaN/invalid rules as the accepted FP helpers.
fn div(a: f64, b: f64) -> f64 {
    let result = a / b;
    if a.is_nan() {
        f64::from_bits(a.to_bits() | 0x8000000000000)
    } else if b.is_nan() {
        f64::from_bits(b.to_bits() | 0x8000000000000)
    } else if result.is_nan() {
        f64::from_bits(0xfff8000000000000)
    } else {
        result
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct EdaAngle {
    value: f64,
}
#[derive(Clone, Copy, Debug)]
pub enum AngleUnit {
    TenthsOfDegree,
    Degrees,
    Radians,
}
impl EdaAngle {
    pub const DEGREES_TO_RADIANS: f64 = core::f64::consts::PI / 180.0;
    pub const fn degrees(value: f64) -> Self {
        Self { value }
    }
    pub fn new(value: f64, unit: AngleUnit) -> Self {
        Self::degrees(match unit {
            AngleUnit::Radians => div(value, Self::DEGREES_TO_RADIANS),
            AngleUnit::TenthsOfDegree => div(value, 10.0),
            AngleUnit::Degrees => value,
        })
    }
    pub fn from_vector(vector: Vector2D) -> GeometryResult<Self> {
        let (x, y) = (vector.x, vector.y);
        let value = if x == 0.0 && y == 0.0 {
            0.0
        } else if y == 0.0 {
            if x >= 0.0 {
                0.0
            } else {
                -180.0
            }
        } else if x == 0.0 {
            if y >= 0.0 {
                90.0
            } else {
                -90.0
            }
        } else if x == y {
            if x >= 0.0 {
                45.0
            } else {
                -180.0 + 45.0
            }
        } else if x == -y {
            if x >= 0.0 {
                -45.0
            } else {
                180.0 - 45.0
            }
        } else {
            return Self::arctan2(y, x);
        };
        Ok(Self::degrees(value))
    }
    pub const fn as_degrees(self) -> f64 {
        self.value
    }
    pub fn as_radians(self) -> f64 {
        fp::mul(self.value, Self::DEGREES_TO_RADIANS)
    }
    pub fn as_tenths_of_degree(self) -> GeometryResult<i32> {
        round_i32(fp::mul(self.value, 10.0))
    }
    pub fn is_zero(self) -> bool {
        self.value == 0.0
    }
    pub fn is_horizontal(self) -> bool {
        self.value == 0.0 || self.value == 180.0
    }
    pub fn is_vertical(self) -> bool {
        self.value == 90.0 || self.value == 270.0
    }
    pub fn is_parallel_to(self, mut angle: Self) -> GeometryResult<bool> {
        let mut normalized = self;
        normalized.normalize90();
        if equals(normalized.value, -90.0)? {
            normalized = ANGLE_90;
        }
        angle.normalize90();
        if equals(angle.value, -90.0)? {
            angle = ANGLE_90;
        }
        equals(normalized.value, angle.value)
    }
    pub fn invert(self) -> Self {
        Self::degrees(-self.value)
    }
    pub fn sin(self) -> f64 {
        let test = self.normalized().value;
        if test == 0.0 || test == 180.0 {
            0.0
        } else if test == 45.0 || test == 135.0 {
            core::f64::consts::FRAC_1_SQRT_2
        } else if test == 225.0 || test == 315.0 {
            -core::f64::consts::FRAC_1_SQRT_2
        } else if test == 90.0 {
            1.0
        } else if test == 270.0 {
            -1.0
        } else {
            glibc_trig::sin(self.as_radians())
        }
    }
    pub fn cos(self) -> f64 {
        let test = self.normalized().value;
        if test == 0.0 {
            1.0
        } else if test == 180.0 {
            -1.0
        } else if test == 90.0 || test == 270.0 {
            0.0
        } else if test == 45.0 || test == 315.0 {
            core::f64::consts::FRAC_1_SQRT_2
        } else if test == 135.0 || test == 225.0 {
            -core::f64::consts::FRAC_1_SQRT_2
        } else {
            glibc_trig::cos(self.as_radians())
        }
    }
    // Unlike sin/cos, these C runtime operations have no accepted portable
    // bit-exact backend yet. Keep the wrapper continuation; never expose a
    // host-dependent answer as portable geometry.
    pub fn tan(self) -> GeometryResult<f64> {
        scalar_libm("libc::tan", self.as_radians())
    }
    pub fn arccos(x: f64) -> GeometryResult<Self> {
        Ok(Self::new(scalar_libm("libc::acos", x)?, AngleUnit::Radians))
    }
    pub fn arcsin(x: f64) -> GeometryResult<Self> {
        Ok(Self::new(scalar_libm("libc::asin", x)?, AngleUnit::Radians))
    }
    pub fn arctan(x: f64) -> GeometryResult<Self> {
        Ok(Self::new(scalar_libm("libc::atan", x)?, AngleUnit::Radians))
    }
    pub fn arctan2(y: f64, x: f64) -> GeometryResult<Self> {
        Ok(Self::new(scalar_atan2(y, x)?, AngleUnit::Radians))
    }
    fn fold_to_two_periods(value: f64, period: f64) -> f64 {
        if !value.is_finite() {
            return 0.0;
        }
        if value.abs() < 2.0 * period {
            return value;
        }
        value % period + period.copysign(value)
    }
    pub fn normalize(&mut self) -> Self {
        self.value = Self::fold_to_two_periods(self.value, 360.0);
        while self.value < -0.0 {
            self.value += 360.0;
        }
        while self.value >= 360.0 {
            self.value -= 360.0;
        }
        *self
    }
    pub fn normalized(self) -> Self {
        let mut value = self;
        value.normalize()
    }
    pub fn normalize_negative(&mut self) -> Self {
        self.value = Self::fold_to_two_periods(self.value, 360.0);
        while self.value <= -360.0 {
            self.value += 360.0;
        }
        while self.value > 0.0 {
            self.value -= 360.0;
        }
        *self
    }
    pub fn normalize90(&mut self) -> Self {
        self.value = Self::fold_to_two_periods(self.value, 180.0);
        while self.value < -90.0 {
            self.value += 180.0;
        }
        while self.value > 90.0 {
            self.value -= 180.0;
        }
        *self
    }
    pub fn normalize180(&mut self) -> Self {
        self.value = Self::fold_to_two_periods(self.value, 360.0);
        while self.value <= -180.0 {
            self.value += 360.0;
        }
        while self.value > 180.0 {
            self.value -= 360.0;
        }
        *self
    }
    pub fn normalize720(&mut self) -> Self {
        self.value = Self::fold_to_two_periods(self.value, 360.0);
        while self.value < -360.0 {
            self.value += 360.0;
        }
        while self.value >= 360.0 {
            self.value -= 360.0;
        }
        *self
    }
    pub fn snapped(self, step: Self) -> GeometryResult<Self> {
        let step = step.value;
        if step <= 0.0 {
            return Ok(self);
        }
        let rounded = round_i32(div(self.value, step))?;
        Ok(Self::degrees(fp::mul(step, f64::from(rounded))))
    }
    pub fn reflect(self, axis: Self) -> Self {
        Self::degrees(fp::sub(fp::mul(2.0, axis.value), self.value))
    }
    pub fn round(self, digits: i32) -> GeometryResult<Self> {
        // Keep both pow calls and the post-KiROUND division in source order.
        let rounded = round_i32(fp::mul(self.value, 10.0_f64.powf(f64::from(digits))))?;
        Ok(Self::degrees(div(
            f64::from(rounded),
            10.0_f64.powf(f64::from(digits)),
        )))
    }
    /// Named forms retain the C++ mutable-self return contract; the Rust traits
    /// below discard that reference because their language signature returns ().
    pub fn add_assign(&mut self, angle: Self) -> &mut Self {
        *self = Self::degrees(fp::add(self.value, angle.value));
        self
    }
    pub fn sub_assign(&mut self, angle: Self) -> &mut Self {
        *self = Self::degrees(fp::sub(self.value, angle.value));
        self
    }
    pub fn abs(self) -> Self {
        Self::degrees(self.value.abs())
    }
}
// UNPORTED(libs.kimath.include.math.application.ui.p001): KiROUND<double, int>
fn round_i32(_value: f64) -> GeometryResult<i32> {
    missing("KiROUND<double, int>", UTIL_HEADER)
}
// UNPORTED(libs.kimath.include.math.application.ui.p001): equals<double>
fn equals(_a: f64, _b: f64) -> GeometryResult<bool> {
    missing("equals<double>", UTIL_HEADER)
}
impl Neg for EdaAngle {
    type Output = Self;
    fn neg(self) -> Self {
        self.invert()
    }
}
impl Add for EdaAngle {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::degrees(fp::add(self.value, rhs.value))
    }
}
impl Sub for EdaAngle {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::degrees(fp::sub(self.value, rhs.value))
    }
}
impl Mul<f64> for EdaAngle {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::degrees(fp::mul(self.value, rhs))
    }
}
impl Div<f64> for EdaAngle {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self::degrees(div(self.value, rhs))
    }
}
impl Div for EdaAngle {
    type Output = f64;
    fn div(self, rhs: Self) -> f64 {
        div(self.value, rhs.value)
    }
}
impl AddAssign for EdaAngle {
    fn add_assign(&mut self, rhs: Self) {
        Self::add_assign(self, rhs);
    }
}
impl SubAssign for EdaAngle {
    fn sub_assign(&mut self, rhs: Self) {
        Self::sub_assign(self, rhs);
    }
}
impl StreamInsert for EdaAngle {
    fn insert(&self, state: &mut StreamState) -> Result<String, StreamError> {
        self.value.insert(state)
    }
}

pub const ANGLE_HORIZONTAL: EdaAngle = EdaAngle::degrees(0.0);
pub const ANGLE_VERTICAL: EdaAngle = EdaAngle::degrees(90.0);
pub const FULL_CIRCLE: EdaAngle = EdaAngle::degrees(360.0);
pub const ANGLE_0: EdaAngle = EdaAngle::degrees(0.0);
pub const ANGLE_45: EdaAngle = EdaAngle::degrees(45.0);
pub const ANGLE_90: EdaAngle = EdaAngle::degrees(90.0);
pub const ANGLE_135: EdaAngle = EdaAngle::degrees(135.0);
pub const ANGLE_180: EdaAngle = EdaAngle::degrees(180.0);
pub const ANGLE_270: EdaAngle = EdaAngle::degrees(270.0);
pub const ANGLE_360: EdaAngle = EdaAngle::degrees(360.0);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdaOrientation {
    angle: EdaAngle,
}
impl EdaOrientation {
    pub fn new(angle: EdaAngle) -> Self {
        let mut result = Self::default();
        result.set_angle(angle);
        result
    }
    pub const fn get_angle(self) -> EdaAngle {
        self.angle
    }
    pub fn set_angle(&mut self, angle: EdaAngle) {
        self.angle = angle;
        self.angle.normalize();
        if self.angle == ANGLE_0 {
            self.angle = ANGLE_0;
        }
    }
    pub fn assign_angle(&mut self, angle: EdaAngle) -> &mut Self {
        self.set_angle(angle);
        self
    }
    pub fn add_assign(&mut self, angle: EdaAngle) -> &mut Self {
        self.set_angle(self.angle + angle);
        self
    }
    pub fn sub_assign(&mut self, angle: EdaAngle) -> &mut Self {
        self.set_angle(self.angle - angle);
        self
    }
    pub fn reflect(self, axis: EdaAngle) -> Self {
        Self::new(self.angle.reflect(axis))
    }
}
impl Add<EdaAngle> for EdaOrientation {
    type Output = Self;
    fn add(self, rhs: EdaAngle) -> Self {
        Self::new(self.angle + rhs)
    }
}
impl Sub<EdaAngle> for EdaOrientation {
    type Output = Self;
    fn sub(self, rhs: EdaAngle) -> Self {
        Self::new(self.angle - rhs)
    }
}
impl Neg for EdaOrientation {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.angle)
    }
}
impl Sub for EdaOrientation {
    type Output = EdaAngle;
    fn sub(self, rhs: Self) -> EdaAngle {
        self.angle - rhs.angle
    }
}
impl AddAssign<EdaAngle> for EdaOrientation {
    fn add_assign(&mut self, rhs: EdaAngle) {
        Self::add_assign(self, rhs);
    }
}
impl SubAssign<EdaAngle> for EdaOrientation {
    fn sub_assign(&mut self, rhs: EdaAngle) {
        Self::sub_assign(self, rhs);
    }
}
impl Add<EdaOrientation> for EdaAngle {
    type Output = EdaOrientation;
    fn add(self, rhs: EdaOrientation) -> EdaOrientation {
        rhs + self
    }
}
impl Sub<EdaOrientation> for EdaAngle {
    type Output = EdaOrientation;
    fn sub(self, rhs: EdaOrientation) -> EdaOrientation {
        EdaOrientation::new(self - rhs.angle)
    }
}
impl StreamInsert for EdaOrientation {
    fn insert(&self, state: &mut StreamState) -> Result<String, StreamError> {
        self.angle.insert(state)
    }
}

// UNPORTED(libs.kimath.include.geometry.application.ui.p001): libc tan/acos/asin/atan backend
fn scalar_libm(upstream: &'static str, _x: f64) -> GeometryResult<f64> {
    missing(upstream, UNIT)
}
// UNPORTED(libs.kimath.include.geometry.application.ui.p001): libc atan2 backend
fn scalar_atan2(_y: f64, _x: f64) -> GeometryResult<f64> {
    missing("libc::atan2", UNIT)
}
