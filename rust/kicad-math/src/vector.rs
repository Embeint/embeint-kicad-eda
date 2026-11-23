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
//! Every other vector operation, including equality, arithmetic, casting constructors and
//! stream output, is still owned by ui.p002 and is deliberately absent rather than guessed.

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
