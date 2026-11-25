// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 (Copyright Angus Johnson 2010-2025), Boost Software
// License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Faithful port of `thirdparty/clipper2/Clipper2Lib/include/clipper2/clipper.core.h` and
//! `clipper.engine.h` at KiCad pin 4cf939778df04d2b747bb2c4714fc5014a7565c7 (Clipper2 2.0.1).
//!
//! Unit `thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p001` owns every
//! body written in those two headers: [`clipper_core`] (points, rectangles, paths, scaling, products,
//! areas, intersections, point in polygon, error reporting), [`stream`] (the `operator<<`
//! insertions) and [`clipper_engine`] (the engine data structures, their constructors, the inline
//! `ClipperBase`, `Clipper64`, `ClipperD` and `PolyPath` members).
//!
//! The port reproduces KiCad's build configuration of the headers exactly:
//!
//! - `USINGZ` is a public compile definition of the `clipper2` target, so every point carries
//!   a `z` coordinate.
//! - KiCad compiles with C++ exceptions, so `DoError` throws a `Clipper2Exception`
//!   ([`ClipperError::Exception`]); the "non-fatal" continuations after `DoError` are not
//!   compiled in that configuration and are therefore not reachable.
//! - `CLIPPER2_HI_PRECISION` and `CLIPPER2_MAX_DECIMAL_PRECISION` are not defined.
//! - The 64-bit GCC/Clang oracle takes the `__int128_t` branches of `ProductsAreEqual` and
//!   `CrossProductSign`. The portable `MultiplyUInt64` branches are ported too
//!   ([`clipper_core::products_are_equal_without_int128`],
//!   [`clipper_core::cross_product_sign_without_int128`]).
//!
//! Where the C++ evaluates undefined behaviour (signed overflow, out-of-range
//! floating-to-integer conversion, out-of-range indexing), the port refuses with
//! [`ClipperError::Undefined`] and never fabricates a value. Floating point follows the
//! oracle's `-ffp-contract=off`: binary64 operations in source order, no fused multiply-add,
//! no host libm.
//!
//! Engine bodies written in `clipper.engine.cpp` belong to the
//! `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001`/`p002` units; header bodies
//! that call them return [`ClipperError::Unported`] at that call, naming the owning unit.

pub mod clipper_core;
pub mod clipper_engine;
pub mod stream;

pub use crate::clipper_core::*;

/// This port unit.
pub const UNIT: &str = "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p001";
/// Owner of `clipper.engine.cpp` lines 1-2271.
pub const ENGINE_SRC_P001: &str = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001";
/// Owner of `clipper.engine.cpp` lines 2272-3337 and `clipper.offset.cpp`.
pub const ENGINE_SRC_P002: &str = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002";

/// `Clipper2Lib::Clipper2Exception`: `what()` returns the description it was built with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clipper2Exception {
    descr: String,
}

impl Clipper2Exception {
    /// `explicit Clipper2Exception(const char* description)`.
    pub fn new(description: &str) -> Self {
        Self {
            descr: description.to_owned(),
        }
    }

    /// `what() const noexcept`.
    pub fn what(&self) -> &str {
        &self.descr
    }
}

/// Every way a ported operation can end other than with its normal result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipperError {
    /// A `Clipper2Exception` thrown by `DoError`. Any reference parameter the C++ updated
    /// before throwing (such as `error_code`) has been updated here too.
    Exception(Clipper2Exception),
    /// `std::bad_function_call`, thrown when an empty `std::function` is invoked.
    BadFunctionCall,
    /// The C++ expression named here has undefined behaviour for these inputs; the port
    /// refuses instead of inventing a value.
    Undefined(&'static str),
    /// The operation reached a function owned by another, not yet ported unit.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
    /// A modelled `std::ostream` request exceeded the portable formatting budget.
    Stream(kicad_math::StreamError),
}

impl From<kicad_math::StreamError> for ClipperError {
    fn from(error: kicad_math::StreamError) -> Self {
        Self::Stream(error)
    }
}

impl ::core::fmt::Display for ClipperError {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        match self {
            Self::Exception(exception) => {
                write!(formatter, "Clipper2Exception: {}", exception.what())
            }
            Self::BadFunctionCall => formatter.write_str("std::bad_function_call"),
            Self::Undefined(what) => write!(formatter, "undefined behaviour: {what}"),
            Self::Unported { upstream, unit } => write!(formatter, "unported {upstream} ({unit})"),
            Self::Stream(error) => write!(formatter, "{error}"),
        }
    }
}

/// Result of a ported operation.
pub type Result<T> = ::core::result::Result<T, ClipperError>;
