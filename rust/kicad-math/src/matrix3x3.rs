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
//! The C++ is a template, but the pinned tree instantiates it only as `MATRIX3x3<double>`
//! (`MATRIX3x3D`, including the class-template-argument-deduced `MATRIX3x3( VECTOR3D, ... )`),
//! and the scalar operators only with `S = double` (from `Inverse`). The operations are
//! therefore implemented for `Matrix3x3<f64>`. Another instantiation would need its own
//! implementation with the C++ conversion rules for that type.
//!
//! Every expression keeps the C++ evaluation order (left-associative sums, no contraction).

use crate::vector::{Vector2, Vector3};
use core::fmt;
use core::ops::Mul;

/// `MATRIX3x3<T>`; `data` is the public `m_data[row][column]`.
#[derive(Clone, Copy, Debug)]
pub struct Matrix3x3<T> {
    pub data: [[T; 3]; 3],
}

/// `MATRIX3x3D`
pub type Matrix3x3D = Matrix3x3<f64>;

impl Default for Matrix3x3<f64> {
    /// `MATRIX3x3()`: initialize all matrix members to zero.
    fn default() -> Self {
        Self::new()
    }
}

impl Matrix3x3<f64> {
    /// `MATRIX3x3()`: initialize all matrix members to zero (`0.0`, positive zero).
    pub const fn new() -> Self {
        Self {
            data: [[0.0; 3]; 3],
        }
    }

    /// `MATRIX3x3( VECTOR3<T> a1, VECTOR3<T> a2, VECTOR3<T> a3 )`: the vectors become rows.
    pub const fn from_vectors(a1: Vector3<f64>, a2: Vector3<f64>, a3: Vector3<f64>) -> Self {
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
        a00: f64,
        a01: f64,
        a02: f64,
        a10: f64,
        a11: f64,
        a12: f64,
        a20: f64,
        a21: f64,
        a22: f64,
    ) -> Self {
        Self {
            data: [[a00, a01, a02], [a10, a11, a12], [a20, a21, a22]],
        }
    }

    /// `SetIdentity()`: the diagonal components are set to 1, all others to 0.
    pub fn set_identity(&mut self) {
        for j in 0..3 {
            for i in 0..3 {
                self.data[i][j] = if i == j { 1.0 } else { 0.0 };
            }
        }
    }

    /// `SetTranslation( VECTOR2<T> )`: sets `m_data[0][2]` and `m_data[1][2]` only.
    pub fn set_translation(&mut self, translation: Vector2<f64>) {
        self.data[0][2] = translation.x;
        self.data[1][2] = translation.y;
    }

    /// `GetTranslation()`
    pub fn get_translation(&self) -> Vector2<f64> {
        // The C++ default-constructs `result` and then assigns both components; the
        // zero initial values are always overwritten.
        Vector2::new(self.data[0][2], self.data[1][2])
    }

    /// `SetRotation( T aAngle )`: sets the upper-left 2x2 block for an anti-clockwise rotation
    /// by `angle` radians; the other members are unchanged.
    ///
    /// `cos` and `sin` are the platform C library's (`f64::cos`/`f64::sin` call the system
    /// libm natively), matching the oracle's `cos( double )` and `sin( double )` on the same
    /// host. See `port/findings/kimath-matrix-libm-sin-cos.md` for the portable limit.
    pub fn set_rotation(&mut self, angle: f64) {
        let cos_value = angle.cos();
        let sin_value = angle.sin();
        self.data[0][0] = cos_value;
        self.data[0][1] = -sin_value;
        self.data[1][0] = sin_value;
        self.data[1][1] = cos_value;
    }

    /// `SetScale( VECTOR2<T> )`: sets the two diagonal scale members only.
    pub fn set_scale(&mut self, scale: Vector2<f64>) {
        self.data[0][0] = scale.x;
        self.data[1][1] = scale.y;
    }

    /// `GetScale()`
    pub fn get_scale(&self) -> Vector2<f64> {
        Vector2::new(self.data[0][0], self.data[1][1])
    }

    /// `Determinant()`, by cofactor expansion along the first row.
    pub fn determinant(&self) -> f64 {
        let m = &self.data;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    /// `Inverse()`: the adjugate times `1.0 / Determinant()`.
    ///
    /// There is no singularity check, as in the C++: a zero determinant gives infinite or NaN
    /// members, which are returned unchanged.
    pub fn inverse(&self) -> Self {
        let m = &self.data;
        let mut result = Self::new();

        result.data[0][0] = m[1][1] * m[2][2] - m[2][1] * m[1][2];
        result.data[0][1] = m[0][2] * m[2][1] - m[2][2] * m[0][1];
        result.data[0][2] = m[0][1] * m[1][2] - m[1][1] * m[0][2];

        result.data[1][0] = m[1][2] * m[2][0] - m[2][2] * m[1][0];
        result.data[1][1] = m[0][0] * m[2][2] - m[2][0] * m[0][2];
        result.data[1][2] = m[0][2] * m[1][0] - m[1][2] * m[0][0];

        result.data[2][0] = m[1][0] * m[2][1] - m[2][0] * m[1][1];
        result.data[2][1] = m[0][1] * m[2][0] - m[2][1] * m[0][0];
        result.data[2][2] = m[0][0] * m[1][1] - m[1][0] * m[0][1];

        result.mul_scalar(1.0 / self.determinant())
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

    /// `operator==`: every member compares equal (IEEE: `-0 == 0`, NaN never equal).
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

    /// `operator*( MATRIX3x3<T>, MATRIX3x3<T> )`: matrix multiplication.
    #[allow(clippy::needless_range_loop, reason = "mirrors the C++ index loops")]
    pub fn mul_matrix(&self, b: &Self) -> Self {
        let a = &self.data;
        let mut result = Self::new();

        for i in 0..3 {
            for j in 0..3 {
                result.data[i][j] =
                    a[i][0] * b.data[0][j] + a[i][1] * b.data[1][j] + a[i][2] * b.data[2][j];
            }
        }

        result
    }

    /// `operator*( MATRIX3x3<T>, VECTOR2<T> )`: multiplication with a 2D vector; the third
    /// component is assumed to be 1.
    pub fn mul_vector2(&self, v: &Vector2<f64>) -> Vector2<f64> {
        let m = &self.data;
        // VECTOR2<T> result( 0, 0 ); then assign both components.
        let mut result = Vector2::new(0.0, 0.0);
        result.x = m[0][0] * v.x + m[0][1] * v.y + m[0][2];
        result.y = m[1][0] * v.x + m[1][1] * v.y + m[1][2];
        result
    }

    /// `operator*( MATRIX3x3<T>, VECTOR3<T> )`
    pub fn mul_vector3(&self, v: &Vector3<f64>) -> Vector3<f64> {
        let m = &self.data;
        // VECTOR3<T> result( 0, 0, 0 ); then assign all components.
        let mut result = Vector3::new(0.0, 0.0, 0.0);
        result.x = m[0][0] * v.x + m[0][1] * v.y + m[0][2] * v.z;
        result.y = m[1][0] * v.x + m[1][1] * v.y + m[1][2] * v.z;
        result.z = m[2][0] * v.x + m[2][1] * v.y + m[2][2] * v.z;
        result
    }

    /// `operator*( MATRIX3x3<T>, S aScalar )` with `S = double`: multiplication with a scalar.
    pub fn mul_scalar(&self, scalar: f64) -> Self {
        let mut result = Self::new();

        for i in 0..3 {
            for j in 0..3 {
                result.data[i][j] = self.data[i][j] * scalar;
            }
        }

        result
    }

    /// `operator*( S aScalar, MATRIX3x3<T> )` with `S = double`: returns `aMatrix * aScalar`.
    pub fn scalar_mul(scalar: f64, matrix: &Self) -> Self {
        matrix.mul_scalar(scalar)
    }

    /// `operator<<( std::ostream&, const MATRIX3x3<T>& )` on a default-state stream: each row
    /// is `"| "`, then each member followed by a space, then `"|"` and `"\n"`.
    pub fn stream_text(&self) -> String {
        let mut text = String::new();

        for row in &self.data {
            text.push_str("| ");

            for value in row {
                text.push_str(&format_stream_double(*value));
                text.push(' ');
            }

            text.push('|');
            text.push('\n');
        }

        text
    }
}

impl PartialEq for Matrix3x3<f64> {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }

    // Keep the C++ operator!= body rather than the default negation (they agree for IEEE
    // doubles, but the port follows the reference definition).
    #[allow(
        clippy::partialeq_ne_impl,
        reason = "ports the separate C++ operator!= body"
    )]
    fn ne(&self, other: &Self) -> bool {
        self.not_equals(other)
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
        Matrix3x3::scalar_mul(self, &rhs)
    }
}

impl fmt::Display for Matrix3x3<f64> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.stream_text())
    }
}

/// `std::ostream << double` with the default stream state: libstdc++ formats through the C
/// library as `%.6g` (precision 6, neither fixed nor scientific, classic "C" locale).
///
/// glibc prints a NaN with its sign bit set as `-nan`, and infinities as `inf`/`-inf`.
/// Finite values use Rust's exact decimal conversion, which rounds exact ties to even as
/// glibc does in the default rounding mode.
pub fn format_stream_double(value: f64) -> String {
    const PRECISION: i32 = 6;
    let sign = if value.is_sign_negative() { "-" } else { "" };

    if value.is_nan() {
        return format!("{sign}nan");
    }

    if value.is_infinite() {
        return format!("{sign}inf");
    }

    if value == 0.0 {
        return format!("{sign}0");
    }

    // %e with precision P - 1 decides the exponent X after rounding to P significant digits.
    let scientific = format!("{:.*e}", (PRECISION - 1) as usize, value.abs());
    let (mantissa, exponent) = match scientific.split_once('e') {
        Some(parts) => parts,
        None => return scientific,
    };
    let exponent: i32 = match exponent.parse() {
        Ok(exponent) => exponent,
        Err(_) => return scientific,
    };
    let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();

    let mut text = String::from(sign);

    if (-4..PRECISION).contains(&exponent) {
        // %f with precision P - 1 - X prints exactly the same P significant digits.
        let (integer, fraction): (Vec<u8>, Vec<u8>) = if exponent >= 0 {
            let split = (exponent + 1) as usize;
            (
                digits.get(..split).unwrap_or(&[]).to_vec(),
                digits.get(split..).unwrap_or(&[]).to_vec(),
            )
        } else {
            let mut fraction = vec![b'0'; (-exponent - 1) as usize];
            fraction.extend_from_slice(&digits);
            (vec![b'0'], fraction)
        };
        push_trimmed(&mut text, &integer, &fraction);
    } else {
        let (first, rest) = digits.split_at(digits.len().min(1));
        push_trimmed(&mut text, first, rest);
        text.push('e');
        text.push(if exponent < 0 { '-' } else { '+' });
        text.push_str(&format!("{:02}", exponent.unsigned_abs()));
    }

    text
}

/// Append `integer[.fraction]` with %g's removal of trailing fraction zeros and of a bare
/// decimal point (no `#` flag).
fn push_trimmed(text: &mut String, integer: &[u8], fraction: &[u8]) {
    text.extend(integer.iter().map(|&digit| char::from(digit)));
    let kept = fraction
        .iter()
        .rposition(|&digit| digit != b'0')
        .map_or(0, |last| last + 1);

    if kept > 0 {
        text.push('.');
        text.extend(fraction.iter().take(kept).map(|&digit| char::from(digit)));
    }
}
