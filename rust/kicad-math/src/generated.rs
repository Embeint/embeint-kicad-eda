// SPDX-License-Identifier: GPL-3.0-or-later
//! GENERATED from the pinned oracle disassembly by scripts/emu.py + scripts/gen_rust.py.
//! Do not edit; regenerate with scripts/gen_sites.sh.
#![allow(clippy::let_and_return)]
use crate::sse2::{add, div, mul, sub};
use kicad_math::{Matrix3x3D, Vector2, Vector3};

/// GENERATED from `sites/determinant-doubleops.trees.json`: site `determinant@DoubleOps 0x489b7-0x48a4b`.
/// Each call is one instruction lane, `op(dst, src)`, in the pinned binary operand order.
pub fn determinant_doubleops_generated(matrix: &Matrix3x3D) -> f64 {
    let m = &matrix.data;
    let v0 = mul(m[1][1], m[2][2]);
    let v1 = mul(m[1][2], m[2][1]);
    let v2 = sub(v0, v1);
    let v3 = mul(v2, m[0][0]);
    let v4 = mul(m[2][2], m[1][0]);
    let v5 = mul(m[1][2], m[2][0]);
    let v6 = sub(v4, v5);
    let v7 = mul(v6, m[0][1]);
    let v8 = sub(v3, v7);
    let v9 = mul(m[2][1], m[1][0]);
    let v10 = mul(m[1][1], m[2][0]);
    let v11 = sub(v9, v10);
    let v12 = mul(v11, m[0][2]);
    let v13 = add(v8, v12);
    v13
}

/// GENERATED from `sites/inverse.trees.json`: site `MATRIX3x3<double>::Inverse() 0x4fbb0-0x4fd29`.
/// Each call is one instruction lane, `op(dst, src)`, in the pinned binary operand order.
pub fn inverse_member(matrix: &Matrix3x3D) -> Matrix3x3D {
    let m = &matrix.data;
    let v0 = mul(m[1][1], m[2][2]);
    let v1 = mul(m[2][1], m[1][2]);
    let v2 = sub(v0, v1);
    let v3 = mul(v2, m[0][0]);
    let v4 = mul(m[2][2], m[1][0]);
    let v5 = mul(m[2][0], m[1][2]);
    let v6 = sub(v4, v5);
    let v7 = mul(v6, m[0][1]);
    let v8 = sub(v3, v7);
    let v9 = mul(m[2][1], m[1][0]);
    let v10 = mul(m[1][1], m[2][0]);
    let v11 = sub(v9, v10);
    let v12 = mul(m[0][2], v11);
    let v13 = add(v8, v12);
    let v14 = div(f64::from_bits(0x3ff0000000000000), v13);
    let v15 = mul(v2, v14);
    let v16 = mul(m[2][1], m[0][2]);
    let v17 = mul(m[2][2], m[0][1]);
    let v18 = sub(v16, v17);
    let v19 = mul(v18, v14);
    let v20 = mul(m[0][1], m[1][2]);
    let v21 = mul(m[1][1], m[0][2]);
    let v22 = sub(v20, v21);
    let v23 = mul(v22, v14);
    let v24 = sub(v5, v4);
    let v25 = mul(v24, v14);
    let v26 = mul(m[0][0], m[2][2]);
    let v27 = mul(m[0][2], m[2][0]);
    let v28 = sub(v26, v27);
    let v29 = mul(v28, v14);
    let v30 = mul(m[1][0], m[0][2]);
    let v31 = mul(m[1][2], m[0][0]);
    let v32 = sub(v30, v31);
    let v33 = mul(v32, v14);
    let v34 = mul(v11, v14);
    let v35 = mul(m[0][1], m[2][0]);
    let v36 = mul(m[2][1], m[0][0]);
    let v37 = sub(v35, v36);
    let v38 = mul(v37, v14);
    let v39 = mul(m[1][1], m[0][0]);
    let v40 = mul(m[1][0], m[0][1]);
    let v41 = sub(v39, v40);
    let v42 = mul(v41, v14);
    Matrix3x3D::from_elements(v15, v19, v23, v25, v29, v33, v34, v38, v42)
}

/// GENERATED from `sites/scalar_mul-doubleops.trees.json`: site `operator*(double, MATRIX3x3<double>)@DoubleOps scalar_mul 0x48274-0x482fb`.
/// Each call is one instruction lane, `op(dst, src)`, in the pinned binary operand order.
pub fn scalar_mul_doubleops(scalar: f64, matrix: &Matrix3x3D) -> Matrix3x3D {
    let m = &matrix.data;
    let v0 = mul(m[0][0], scalar);
    let v1 = mul(m[0][1], scalar);
    let v2 = mul(m[0][2], scalar);
    let v3 = mul(m[1][0], scalar);
    let v4 = mul(m[1][1], scalar);
    let v5 = mul(m[1][2], scalar);
    let v6 = mul(scalar, m[2][0]);
    let v7 = mul(scalar, m[2][1]);
    let v8 = mul(scalar, m[2][2]);
    Matrix3x3D::from_elements(v0, v1, v2, v3, v4, v5, v6, v7, v8)
}

/// GENERATED from `sites/mul_scalar-doubleops.trees.json`: site `operator*(MATRIX3x3<double>, double)@DoubleOps mul_scalar 0x49605-0x49675`.
/// Each call is one instruction lane, `op(dst, src)`, in the pinned binary operand order.
pub fn mul_scalar_doubleops(matrix: &Matrix3x3D, scalar: f64) -> Matrix3x3D {
    let m = &matrix.data;
    let v0 = mul(m[0][0], scalar);
    let v1 = mul(m[0][1], scalar);
    let v2 = mul(m[0][2], scalar);
    let v3 = mul(m[1][0], scalar);
    let v4 = mul(m[1][1], scalar);
    let v5 = mul(m[1][2], scalar);
    let v6 = mul(scalar, m[2][0]);
    let v7 = mul(scalar, m[2][1]);
    let v8 = mul(scalar, m[2][2]);
    Matrix3x3D::from_elements(v0, v1, v2, v3, v4, v5, v6, v7, v8)
}

/// GENERATED from `sites/mul_vector2-doubleops.trees.json`: site `operator*(MATRIX3x3<double>, VECTOR2<double>)@DoubleOps 0x49430-0x494a7`.
/// Each call is one instruction lane, `op(dst, src)`, in the pinned binary operand order.
pub fn mul_vector2_doubleops(matrix: &Matrix3x3D, v: &Vector2<f64>) -> Vector2<f64> {
    let m = &matrix.data;
    let v0 = mul(m[0][1], v.y);
    let v1 = mul(m[0][0], v.x);
    let v2 = add(v0, v1);
    let v3 = add(v2, m[0][2]);
    let v4 = mul(m[1][0], v.x);
    let v5 = mul(m[1][1], v.y);
    let v6 = add(v4, v5);
    let v7 = add(v6, m[1][2]);
    Vector2::new(v3, v7)
}

/// GENERATED from `sites/mul_vector3-doubleops.trees.json`: site `operator*(MATRIX3x3<double>, VECTOR3<double>)@DoubleOps 0x48c8b-0x48d40`.
/// Each call is one instruction lane, `op(dst, src)`, in the pinned binary operand order.
pub fn mul_vector3_doubleops(matrix: &Matrix3x3D, v: &Vector3<f64>) -> Vector3<f64> {
    let m = &matrix.data;
    let v0 = mul(v.x, m[0][0]);
    let v1 = mul(v.y, m[0][1]);
    let v2 = add(v0, v1);
    let v3 = mul(v.z, m[0][2]);
    let v4 = add(v2, v3);
    let v5 = mul(m[1][0], v.x);
    let v6 = mul(m[1][1], v.y);
    let v7 = add(v5, v6);
    let v8 = mul(m[1][2], v.z);
    let v9 = add(v7, v8);
    let v10 = mul(m[2][0], v.x);
    let v11 = mul(m[2][1], v.y);
    let v12 = add(v10, v11);
    let v13 = mul(m[2][2], v.z);
    let v14 = add(v12, v13);
    Vector3::new(v4, v9, v14)
}

/// GENERATED from `sites/mul_matrix-doubleops.trees.json`: site `operator*(MATRIX3x3<double>, MATRIX3x3<double>)@DoubleOps 0x49041-0x4923a`.
/// Each call is one instruction lane, `op(dst, src)`, in the pinned binary operand order.
pub fn mul_matrix_doubleops(a: &Matrix3x3D, b: &Matrix3x3D) -> Matrix3x3D {
    let (a, b) = (&a.data, &b.data);
    let v0 = mul(a[0][1], b[1][0]);
    let v1 = mul(b[0][0], a[0][0]);
    let v2 = add(v0, v1);
    let v3 = mul(a[0][2], b[2][0]);
    let v4 = add(v2, v3);
    let v5 = mul(a[0][0], b[0][1]);
    let v6 = mul(b[1][1], a[0][1]);
    let v7 = add(v5, v6);
    let v8 = mul(a[0][2], b[2][1]);
    let v9 = add(v7, v8);
    let v10 = mul(a[0][1], b[1][2]);
    let v11 = mul(a[0][0], b[0][2]);
    let v12 = add(v10, v11);
    let v13 = mul(a[0][2], b[2][2]);
    let v14 = add(v12, v13);
    let v15 = mul(a[1][1], b[1][0]);
    let v16 = mul(a[1][0], b[0][0]);
    let v17 = add(v15, v16);
    let v18 = mul(a[1][2], b[2][0]);
    let v19 = add(v17, v18);
    let v20 = mul(a[1][2], b[2][1]);
    let v21 = mul(a[1][0], b[0][1]);
    let v22 = mul(b[1][1], a[1][1]);
    let v23 = add(v21, v22);
    let v24 = add(v20, v23);
    let v25 = mul(a[1][2], b[2][2]);
    let v26 = mul(a[1][1], b[1][2]);
    let v27 = mul(b[0][2], a[1][0]);
    let v28 = add(v26, v27);
    let v29 = add(v25, v28);
    let v30 = mul(b[1][0], a[2][1]);
    let v31 = mul(b[0][0], a[2][0]);
    let v32 = add(v30, v31);
    let v33 = mul(b[2][0], a[2][2]);
    let v34 = add(v32, v33);
    let v35 = mul(b[1][1], a[2][1]);
    let v36 = mul(b[0][1], a[2][0]);
    let v37 = add(v35, v36);
    let v38 = mul(b[2][1], a[2][2]);
    let v39 = add(v37, v38);
    let v40 = mul(b[1][2], a[2][1]);
    let v41 = mul(b[0][2], a[2][0]);
    let v42 = add(v40, v41);
    let v43 = mul(b[2][2], a[2][2]);
    let v44 = add(v42, v43);
    Matrix3x3D::from_elements(v4, v9, v14, v19, v24, v29, v34, v39, v44)
}

