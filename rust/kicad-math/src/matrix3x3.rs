// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2012 Torsten Hueter, torstenhtr <at> gmx.de
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! Port of `libs/kimath/include/math/matrix3x3.h` at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! `MATRIX3x3` describes a general 3x3 matrix. Any linear transformation in 2D can be
//! represented by a homogeneous 3x3 transformation matrix; affine transformations extend the
//! 2D vector `(x, y)` to `(x, y, 1)`. Transformations combine by matrix multiplication:
//! `A * (B * x) = (A * B) * x`.
//!
//! The C++ is a template. The pinned tree instantiates it as `MATRIX3x3<double>`
//! (`MATRIX3x3D`, the application type) and, in `qa/tests/libs/kimath/math/test_matrix3x3.cpp`,
//! as `MATRIX3x3<int>` (deduced from `VECTOR3I` rows, multiplied by a `VECTOR3I` and by the
//! `int` 5). The scalar operators are instantiated with `S = double` (from `Inverse`) and
//! `S = int` (the QA test). Both element types are implemented here:
//!
//! - `Matrix3x3<f64>`: IEEE binary64 arithmetic in the C++ evaluation order (left-associative
//!   sums, no contraction). It never fails. When several operands are NaN, which NaN survives
//!   is not defined by the source; it is the compiled oracle's choice. `Determinant`,
//!   `Inverse` and the `double` products therefore evaluate generated per-site code
//!   (`crate::generated`, derived from oracle sha256 `0bac7567…`, GCC 13.3 `-O3
//!   -fno-fast-math -ffp-contract=off`, x86-64) on explicit SSE2 NaN selection
//!   (`crate::sse2`): the same operations, operands and rounding as the C++ expression, with
//!   each commutative `+`/`*` in the binary's operand order and invalid operations giving
//!   `fff8000000000000`, identically on every target. See
//!   `port/findings/kimath-matrix-mixed-nan-payloads.md`.
//! - `Matrix3x3<i32>`: `int` arithmetic. Signed overflow and the conversion of a NaN or
//!   out-of-range `double` to `int` are undefined behaviour in C++; those inputs return a
//!   [`MatrixError`] instead of a value. Every defined input returns the C++ result.

use crate::generated;
use crate::glibc_trig;
use crate::stream::{insert_str, StreamError, StreamInsert, StreamState};
use crate::vector::{Vector2, Vector3};
use core::ops::Mul;

/// `MATRIX3x3<T>`; `data` is the public `m_data[row][column]`.
#[derive(Clone, Copy, Debug)]
pub struct Matrix3x3<T> {
    pub data: [[T; 3]; 3],
}

/// `MATRIX3x3D`
pub type Matrix3x3D = Matrix3x3<f64>;

/// An input for which the C++ `MATRIX3x3<int>` operation has undefined behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixError {
    /// Signed `int` overflow in the named C++ operation.
    Overflow { upstream: &'static str },
    /// A NaN or out-of-range `double` converted to `int` in the named C++ operation.
    Conversion { upstream: &'static str },
}

impl core::fmt::Display for MatrixError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Overflow { upstream } => write!(formatter, "{upstream} overflows int"),
            Self::Conversion { upstream } => {
                write!(formatter, "{upstream} converts a value outside int")
            }
        }
    }
}

/// An element type with the C++ conversions of the literals `0.0` and `1.0`.
pub trait MatrixElement: Copy + PartialEq + StreamInsert {
    /// `T( 0.0 )`
    const ZERO: Self;
    /// `T( 1.0 )`
    const ONE: Self;
}

impl MatrixElement for f64 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
}

impl MatrixElement for i32 {
    const ZERO: Self = 0;
    const ONE: Self = 1;
}

impl<T: MatrixElement> Default for Matrix3x3<T> {
    /// `MATRIX3x3()`: initialize all matrix members to zero.
    fn default() -> Self {
        Self::new()
    }
}

/// Bodies whose behaviour is the same for every element type.
impl<T: MatrixElement> Matrix3x3<T> {
    /// `MATRIX3x3()`: initialize all matrix members to zero (`0.0` converted to `T`).
    pub const fn new() -> Self {
        Self {
            data: [[T::ZERO; 3]; 3],
        }
    }

    /// `MATRIX3x3( VECTOR3<T> a1, VECTOR3<T> a2, VECTOR3<T> a3 )`: the vectors become rows.
    pub const fn from_vectors(a1: Vector3<T>, a2: Vector3<T>, a3: Vector3<T>) -> Self {
        Self {
            data: [[a1.x, a1.y, a1.z], [a2.x, a2.y, a2.z], [a3.x, a3.y, a3.z]],
        }
    }

    /// `MATRIX3x3( T a00, ..., T a22 )`: initialize with the given members, row by row.
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the nine-argument C++ constructor"
    )]
    pub const fn from_elements(
        a00: T,
        a01: T,
        a02: T,
        a10: T,
        a11: T,
        a12: T,
        a20: T,
        a21: T,
        a22: T,
    ) -> Self {
        Self {
            data: [[a00, a01, a02], [a10, a11, a12], [a20, a21, a22]],
        }
    }

    /// `SetIdentity()`: the diagonal components are set to 1, all others to 0.
    pub fn set_identity(&mut self) {
        for j in 0..3 {
            for i in 0..3 {
                self.data[i][j] = if i == j { T::ONE } else { T::ZERO };
            }
        }
    }

    /// `SetTranslation( VECTOR2<T> )`: sets `m_data[0][2]` and `m_data[1][2]` only.
    pub fn set_translation(&mut self, translation: Vector2<T>) {
        self.data[0][2] = translation.x;
        self.data[1][2] = translation.y;
    }

    /// `GetTranslation()`
    pub fn get_translation(&self) -> Vector2<T> {
        // The C++ default-constructs `result` and then assigns both components; the
        // zero initial values are always overwritten.
        Vector2::new(self.data[0][2], self.data[1][2])
    }

    /// `SetScale( VECTOR2<T> )`: sets the two diagonal scale members only.
    pub fn set_scale(&mut self, scale: Vector2<T>) {
        self.data[0][0] = scale.x;
        self.data[1][1] = scale.y;
    }

    /// `GetScale()`
    pub fn get_scale(&self) -> Vector2<T> {
        Vector2::new(self.data[0][0], self.data[1][1])
    }

    /// `Transpose()`
    pub fn transpose(&self) -> Self {
        let mut result = Self::new();

        for i in 0..3 {
            for j in 0..3 {
                result.data[j][i] = self.data[i][j];
            }
        }

        result
    }

    /// `operator==`: every member compares equal (for doubles, IEEE: `-0 == 0`, NaN never
    /// equal).
    pub fn equals(&self, other: &Self) -> bool {
        let (a, b) = (&other.data, &self.data);
        a[0][0] == b[0][0]
            && a[0][1] == b[0][1]
            && a[0][2] == b[0][2]
            && a[1][0] == b[1][0]
            && a[1][1] == b[1][1]
            && a[1][2] == b[1][2]
            && a[2][0] == b[2][0]
            && a[2][1] == b[2][1]
            && a[2][2] == b[2][2]
    }

    /// `operator!=`: any member compares unequal.
    pub fn not_equals(&self, other: &Self) -> bool {
        let (a, b) = (&other.data, &self.data);
        a[0][0] != b[0][0]
            || a[0][1] != b[0][1]
            || a[0][2] != b[0][2]
            || a[1][0] != b[1][0]
            || a[1][1] != b[1][1]
            || a[1][2] != b[1][2]
            || a[2][0] != b[2][0]
            || a[2][1] != b[2][1]
            || a[2][2] != b[2][2]
    }

    /// `operator<<( std::ostream&, const MATRIX3x3<T>& )` on a stream in `state`: each row is
    /// `"| "`, then each member followed by `" "`, then `"|"` and `"\n"`. Every insertion uses
    /// and updates `state` as the C++ stream does (only the first `"| "` sees a nonzero
    /// width, because every insertion resets it). Resource limits are validated before
    /// any insertion, so a rejected state remains unchanged.
    pub fn stream_text(&self, state: &mut StreamState) -> Result<String, StreamError> {
        state.validate()?;
        let mut text = String::new();

        for row in &self.data {
            text.push_str(&insert_str("| ", state)?);

            for value in row {
                text.push_str(&value.insert(state)?);
                text.push_str(&insert_str(" ", state)?);
            }

            text.push_str(&insert_str("|", state)?);
            text.push_str(&insert_str("\n", state)?);
        }

        Ok(text)
    }
}

impl<T: MatrixElement> PartialEq for Matrix3x3<T> {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }

    // Keep the C++ operator!= body rather than the default negation (they agree, but the
    // port follows the reference definition).
    #[allow(
        clippy::partialeq_ne_impl,
        reason = "ports the separate C++ operator!= body"
    )]
    fn ne(&self, other: &Self) -> bool {
        self.not_equals(other)
    }
}

impl Matrix3x3<f64> {
    /// `SetRotation( T aAngle )`: sets the upper-left 2x2 block for an anti-clockwise rotation
    /// by `angle` radians; the other members are unchanged.
    ///
    /// GCC combines the C++'s `cos( aAngle )` and `sin( aAngle )` into one `sincos` call; the
    /// reference is glibc 2.39's baseline `__sincos_sse2`, which `glibc_trig::sincos`
    /// reproduces bit for bit on every target (no host libm). See
    /// `port/findings/kimath-matrix-libm-sin-cos.md`.
    pub fn set_rotation(&mut self, angle: f64) {
        let (sin_value, cos_value) = glibc_trig::sincos(angle);
        self.data[0][0] = cos_value;
        self.data[0][1] = -sin_value;
        self.data[1][0] = sin_value;
        self.data[1][1] = cos_value;
    }

    /// `Determinant()`, by cofactor expansion along the first row:
    ///
    /// `m00 * (m11 * m22 - m12 * m21) - m01 * (m10 * m22 - m12 * m20)
    ///  + m02 * (m10 * m21 - m11 * m20)`
    ///
    /// evaluated with the compiled oracle's NaN operand priority (module documentation).
    pub fn determinant(&self) -> f64 {
        generated::determinant_doubleops_generated(self)
    }

    /// `Inverse()`: the adjugate times `1.0 / Determinant()`.
    ///
    /// There is no singularity check, as in the C++: a zero determinant gives infinite or NaN
    /// members, which are returned unchanged. The compiled `Inverse` computes its own
    /// determinant inline and shares cofactors with it, so it is its own generated site rather
    /// than [`Self::determinant`] followed by [`Self::mul_scalar`]; the finite results are the
    /// same operations on the same operands.
    pub fn inverse(&self) -> Self {
        generated::inverse_member(self)
    }

    /// `operator*( MATRIX3x3<T>, MATRIX3x3<T> )`: matrix multiplication,
    /// `result[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j]`.
    pub fn mul_matrix(&self, b: &Self) -> Self {
        generated::mul_matrix_doubleops(self, b)
    }

    /// `operator*( MATRIX3x3<T>, VECTOR2<T> )`: multiplication with a 2D vector; the third
    /// component is assumed to be 1: `x = m00 * v.x + m01 * v.y + m02`, likewise `y`.
    pub fn mul_vector2(&self, v: &Vector2<f64>) -> Vector2<f64> {
        generated::mul_vector2_doubleops(self, v)
    }

    /// `operator*( MATRIX3x3<T>, VECTOR3<T> )`: `x = m00 * v.x + m01 * v.y + m02 * v.z`,
    /// likewise `y` and `z`.
    pub fn mul_vector3(&self, v: &Vector3<f64>) -> Vector3<f64> {
        generated::mul_vector3_doubleops(self, v)
    }

    /// `operator*( MATRIX3x3<T>, S aScalar )` with `S = double`: every member times the scalar.
    pub fn mul_scalar(&self, scalar: f64) -> Self {
        generated::mul_scalar_doubleops(self, scalar)
    }

    /// `operator*( MATRIX3x3<T>, S aScalar )` with `S = int`: the `int` converts exactly to
    /// `double` before each multiplication. The converted scalar is never a NaN, so each
    /// product meets at most one NaN and its result does not depend on operand order.
    pub fn mul_scalar_int(&self, scalar: i32) -> Self {
        self.mul_scalar(f64::from(scalar))
    }

    /// `operator*( S aScalar, MATRIX3x3<T> )` with `S = double`: returns `aMatrix * aScalar`.
    /// The oracle inlines this operator separately, so it has its own generated site.
    pub fn scalar_mul(scalar: f64, matrix: &Self) -> Self {
        generated::scalar_mul_doubleops(scalar, matrix)
    }

    /// `operator*( S aScalar, MATRIX3x3<T> )` with `S = int`: returns `aMatrix * aScalar`.
    pub fn scalar_mul_int(scalar: i32, matrix: &Self) -> Self {
        matrix.mul_scalar_int(scalar)
    }
}

/// Checked `int` arithmetic for one named C++ operation.
#[derive(Clone, Copy)]
struct IntOp(&'static str);

impl IntOp {
    fn overflow(self) -> MatrixError {
        MatrixError::Overflow { upstream: self.0 }
    }

    fn mul(self, a: i32, b: i32) -> Result<i32, MatrixError> {
        a.checked_mul(b).ok_or(self.overflow())
    }

    fn add(self, a: i32, b: i32) -> Result<i32, MatrixError> {
        a.checked_add(b).ok_or(self.overflow())
    }

    fn sub(self, a: i32, b: i32) -> Result<i32, MatrixError> {
        a.checked_sub(b).ok_or(self.overflow())
    }

    /// `a * b - c * d`
    fn cross(self, a: i32, b: i32, c: i32, d: i32) -> Result<i32, MatrixError> {
        self.sub(self.mul(a, b)?, self.mul(c, d)?)
    }

    /// `a * x + b * y + c * z`, left to right.
    fn dot3(self, a: i32, x: i32, b: i32, y: i32, c: i32, z: i32) -> Result<i32, MatrixError> {
        let sum = self.add(self.mul(a, x)?, self.mul(b, y)?)?;
        self.add(sum, self.mul(c, z)?)
    }
}

/// `int( value )` for a `double` result: truncation toward zero, defined only when the
/// truncated value is representable (NaN and out-of-range values are undefined behaviour).
fn double_to_int(value: f64, upstream: &'static str) -> Result<i32, MatrixError> {
    if value > -2_147_483_649.0 && value < 2_147_483_648.0 {
        Ok(value as i32)
    } else {
        Err(MatrixError::Conversion { upstream })
    }
}

const INT_DETERMINANT: IntOp = IntOp("MATRIX3x3<int>::Determinant");
const INT_INVERSE: IntOp = IntOp("MATRIX3x3<int>::Inverse");
const INT_MUL_MATRIX: IntOp = IntOp("operator*( MATRIX3x3<int>, MATRIX3x3<int> )");
const INT_MUL_VECTOR2: IntOp = IntOp("operator*( MATRIX3x3<int>, VECTOR2<int> )");
const INT_MUL_VECTOR3: IntOp = IntOp("operator*( MATRIX3x3<int>, VECTOR3<int> )");
const INT_MUL_SCALAR: IntOp = IntOp("operator*( MATRIX3x3<int>, int )");
const INT_MUL_DOUBLE: &str = "operator*( MATRIX3x3<int>, double )";

impl Matrix3x3<i32> {
    /// `SetRotation( int aAngle )`: `T cosValue = cos( aAngle )` evaluates the `double`
    /// cosine of the integer angle and truncates it to `int` (always in [-1, 1], so defined);
    /// likewise for the sine. As for `double`, the pair is one glibc `sincos` call
    /// (`glibc_trig::sincos`). Other members are unchanged.
    pub fn set_rotation(&mut self, angle: i32) {
        let (sin_double, cos_double) = glibc_trig::sincos(f64::from(angle));
        // Truncation of a value in [-1, 1] toward zero.
        let cos_value = cos_double as i32;
        let sin_value = sin_double as i32;
        self.data[0][0] = cos_value;
        self.data[0][1] = -sin_value;
        self.data[1][0] = sin_value;
        self.data[1][1] = cos_value;
    }

    /// `Determinant()` in `int` arithmetic.
    pub fn determinant(&self) -> Result<i32, MatrixError> {
        let m = &self.data;
        let op = INT_DETERMINANT;
        let first = op.mul(m[0][0], op.cross(m[1][1], m[2][2], m[1][2], m[2][1])?)?;
        let second = op.mul(m[0][1], op.cross(m[1][0], m[2][2], m[1][2], m[2][0])?)?;
        let third = op.mul(m[0][2], op.cross(m[1][0], m[2][1], m[1][1], m[2][0])?)?;
        op.add(op.sub(first, second)?, third)
    }

    /// `Inverse()`: `int` cofactors, then `operator*( MATRIX3x3<int>, double )` with
    /// `1.0 / Determinant()`, truncating each product back to `int`. A singular matrix makes
    /// every member `±inf` or NaN before the conversion, which is undefined in C++.
    pub fn inverse(&self) -> Result<Self, MatrixError> {
        let m = &self.data;
        let op = INT_INVERSE;
        let mut result = Self::new();

        result.data[0][0] = op.cross(m[1][1], m[2][2], m[2][1], m[1][2])?;
        result.data[0][1] = op.cross(m[0][2], m[2][1], m[2][2], m[0][1])?;
        result.data[0][2] = op.cross(m[0][1], m[1][2], m[1][1], m[0][2])?;

        result.data[1][0] = op.cross(m[1][2], m[2][0], m[2][2], m[1][0])?;
        result.data[1][1] = op.cross(m[0][0], m[2][2], m[2][0], m[0][2])?;
        result.data[1][2] = op.cross(m[0][2], m[1][0], m[1][2], m[0][0])?;

        result.data[2][0] = op.cross(m[1][0], m[2][1], m[2][0], m[1][1])?;
        result.data[2][1] = op.cross(m[0][1], m[2][0], m[2][1], m[0][0])?;
        result.data[2][2] = op.cross(m[0][0], m[1][1], m[1][0], m[0][1])?;

        result.mul_scalar_double(1.0 / f64::from(self.determinant()?))
    }

    /// `operator*( MATRIX3x3<T>, MATRIX3x3<T> )` in `int` arithmetic.
    #[allow(clippy::needless_range_loop, reason = "mirrors the C++ index loops")]
    pub fn mul_matrix(&self, b: &Self) -> Result<Self, MatrixError> {
        let a = &self.data;
        let mut result = Self::new();

        for i in 0..3 {
            for j in 0..3 {
                result.data[i][j] = INT_MUL_MATRIX.dot3(
                    a[i][0],
                    b.data[0][j],
                    a[i][1],
                    b.data[1][j],
                    a[i][2],
                    b.data[2][j],
                )?;
            }
        }

        Ok(result)
    }

    /// `operator*( MATRIX3x3<T>, VECTOR2<T> )` in `int` arithmetic (implicit z = 1).
    pub fn mul_vector2(&self, v: &Vector2<i32>) -> Result<Vector2<i32>, MatrixError> {
        let m = &self.data;
        let op = INT_MUL_VECTOR2;
        let mut result = Vector2::new(0, 0);
        result.x = op.add(
            op.add(op.mul(m[0][0], v.x)?, op.mul(m[0][1], v.y)?)?,
            m[0][2],
        )?;
        result.y = op.add(
            op.add(op.mul(m[1][0], v.x)?, op.mul(m[1][1], v.y)?)?,
            m[1][2],
        )?;
        Ok(result)
    }

    /// `operator*( MATRIX3x3<T>, VECTOR3<T> )` in `int` arithmetic.
    pub fn mul_vector3(&self, v: &Vector3<i32>) -> Result<Vector3<i32>, MatrixError> {
        let m = &self.data;
        let op = INT_MUL_VECTOR3;
        let mut result = Vector3::new(0, 0, 0);
        result.x = op.dot3(m[0][0], v.x, m[0][1], v.y, m[0][2], v.z)?;
        result.y = op.dot3(m[1][0], v.x, m[1][1], v.y, m[1][2], v.z)?;
        result.z = op.dot3(m[2][0], v.x, m[2][1], v.y, m[2][2], v.z)?;
        Ok(result)
    }

    /// `operator*( MATRIX3x3<T>, S aScalar )` with `S = int`.
    pub fn mul_scalar(&self, scalar: i32) -> Result<Self, MatrixError> {
        let mut result = Self::new();

        for i in 0..3 {
            for j in 0..3 {
                result.data[i][j] = INT_MUL_SCALAR.mul(self.data[i][j], scalar)?;
            }
        }

        Ok(result)
    }

    /// `operator*( MATRIX3x3<T>, S aScalar )` with `S = double`: each `int` member times the
    /// `double`, truncated back to `int`.
    pub fn mul_scalar_double(&self, scalar: f64) -> Result<Self, MatrixError> {
        let mut result = Self::new();

        for i in 0..3 {
            for j in 0..3 {
                result.data[i][j] =
                    double_to_int(f64::from(self.data[i][j]) * scalar, INT_MUL_DOUBLE)?;
            }
        }

        Ok(result)
    }

    /// `operator*( S aScalar, MATRIX3x3<T> )` with `S = int`: returns `aMatrix * aScalar`.
    pub fn scalar_mul(scalar: i32, matrix: &Self) -> Result<Self, MatrixError> {
        matrix.mul_scalar(scalar)
    }

    /// `operator*( S aScalar, MATRIX3x3<T> )` with `S = double`: returns `aMatrix * aScalar`.
    pub fn scalar_mul_double(scalar: f64, matrix: &Self) -> Result<Self, MatrixError> {
        matrix.mul_scalar_double(scalar)
    }
}

impl Mul for Matrix3x3<f64> {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        self.mul_matrix(&rhs)
    }
}

impl Mul<Vector2<f64>> for Matrix3x3<f64> {
    type Output = Vector2<f64>;

    fn mul(self, rhs: Vector2<f64>) -> Vector2<f64> {
        self.mul_vector2(&rhs)
    }
}

impl Mul<Vector3<f64>> for Matrix3x3<f64> {
    type Output = Vector3<f64>;

    fn mul(self, rhs: Vector3<f64>) -> Vector3<f64> {
        self.mul_vector3(&rhs)
    }
}

impl Mul<f64> for Matrix3x3<f64> {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self {
        self.mul_scalar(rhs)
    }
}

impl Mul<Matrix3x3<f64>> for f64 {
    type Output = Matrix3x3<f64>;

    fn mul(self, rhs: Matrix3x3<f64>) -> Matrix3x3<f64> {
        Matrix3x3::<f64>::scalar_mul(self, &rhs)
    }
}
