// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 (Copyright Angus Johnson 2010-2025), Boost Software
// License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

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
//!   ([`ClipperError::Exception`]). The "non-fatal" continuations after each `DoError` call
//!   are still compiled, but they are unreachable because `DoError` throws first.
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
//! Retained state is bounded by a host-supplied [`Budget`] ([`ClipperError::ResourceLimit`]);
//! every allocation sized by caller input is fallible ([`ClipperError::Allocation`]). Neither
//! changes the result of a valid input within the budget.
//!
//! Engine bodies written in `clipper.engine.cpp` belong to the
//! `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001`/`p002` units. Lines 1-2271 (unit
//! p001) are ported in [`engine_p001`], lines 2272-3337 in [`engine_p002`] and
//! `clipper.offset.cpp` in [`offset`] (unit p002). `clipper.rectclip.cpp` is ported in
//! [`rectclip`] (unit src p003).
//!
//! Unit `thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002` owns the
//! remaining headers: [`clipper`] (`clipper.h`), [`clipper_minkowski`]
//! (`clipper.minkowski.h`), [`clipper_triangulation`] (`clipper.triangulation.h` and
//! `clipper.version.h`), and the `clipper.offset.h`/`clipper.rectclip.h` declarations co-ported
//! in [`offset`] and [`rectclip`]. `Triangulate`'s body (`clipper.triangulation.cpp`, which
//! KiCad does not compile) returns [`ClipperError::Unported`] naming its owning unit.

use std::borrow::Cow;

pub mod clipper;
pub mod clipper_core;
pub mod clipper_engine;
pub mod clipper_minkowski;
pub mod clipper_triangulation;
pub mod engine_p001;
pub mod engine_p002;
pub mod offset;
pub mod rectclip;
pub mod stream;

pub use crate::clipper_core::*;

/// This port unit.
pub const UNIT: &str = "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p001";
/// Owner of `clipper.engine.cpp` lines 1-2271.
pub const ENGINE_SRC_P001: &str = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001";
/// Owner of `clipper.engine.cpp` lines 2272-3337 and `clipper.offset.cpp`.
pub const ENGINE_SRC_P002: &str = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002";
/// Owner of `clipper.h`, `clipper.minkowski.h`, `clipper.offset.h`, `clipper.rectclip.h`,
/// `clipper.triangulation.h` and `clipper.version.h`.
pub const INCLUDE_P002: &str = clipper::UNIT;

/// `Clipper2Lib::Clipper2Exception`: `what()` returns the description it was built with.
///
/// Not `Clone`: copying a caller-sized description would be an infallible allocation.
/// The reference's own `DoError` messages are borrowed and never allocate.
#[derive(Debug, PartialEq, Eq)]
pub struct Clipper2Exception {
    descr: Cow<'static, str>,
}

impl Clipper2Exception {
    /// `DoError`'s fixed reference messages, which contain no NUL.
    pub(crate) const fn from_static(description: &'static str) -> Self {
        Self {
            descr: Cow::Borrowed(description),
        }
    }

    /// `explicit Clipper2Exception(const char* description)`: `descr_` is built by
    /// `std::string(const char*)`, which stops at the first NUL, so a description with an
    /// embedded NUL keeps only the text before it. The copy is sized by the caller, so it
    /// is reserved fallibly and fails with [`ClipperError::Allocation`] instead of aborting.
    pub fn new(description: &str) -> Result<Self> {
        // '\0' is ASCII, so its byte index is a character boundary.
        let c_string = match description.find('\0') {
            Some(end) => &description[..end],
            None => description,
        };
        let mut descr = String::new();
        descr
            .try_reserve_exact(c_string.len())
            .map_err(|_| ClipperError::Allocation)?;
        descr.push_str(c_string);
        Ok(Self {
            descr: Cow::Owned(descr),
        })
    }

    /// `what() const noexcept`.
    pub fn what(&self) -> &str {
        &self.descr
    }
}

/// Every way a ported operation can end other than with its normal result.
#[derive(Debug, PartialEq, Eq)]
pub enum ClipperError {
    /// A `Clipper2Exception` thrown by `DoError`. Any reference parameter the C++ updated
    /// before throwing (such as `error_code`) has been updated here too.
    Exception(Clipper2Exception),
    /// `std::bad_function_call`, thrown when an empty `std::function` is invoked.
    BadFunctionCall,
    /// An exception thrown by a caller-supplied callback (a fallible `ZCallback64` of
    /// `ClipperOffset` or a `DeltaCallback64`), identified by a value the callback chose. It
    /// propagates unchanged, leaving the state the C++ leaves when that exception unwinds.
    Callback(u64),
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
    /// Retained state would exceed the host's [`Budget`]; nothing was changed.
    ResourceLimit,
    /// A fallible allocation sized by caller input failed; nothing was changed.
    Allocation,
}

/// Host limits on the state a ported object retains across calls, so repeated caller
/// operations cannot grow memory without bound. Exceeding a limit is
/// [`ClipperError::ResourceLimit`], distinct from undefined reference input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Live `PolyPath` nodes in one tree, including its root.
    pub nodes: usize,
    /// Points held by the live nodes' polygons in one tree.
    pub points: usize,
}

/// Reserve room for `additional` more elements without aborting on failure.
pub(crate) fn reserve<T>(v: &mut Vec<T>, additional: usize) -> Result<()> {
    v.try_reserve(additional)
        .map_err(|_| ClipperError::Allocation)
}

/// `Vec::push` after a fallible reservation.
pub(crate) fn push<T>(v: &mut Vec<T>, value: T) -> Result<()> {
    reserve(v, 1)?;
    v.push(value);
    Ok(())
}

/// An empty vector with fallible room for `capacity` elements.
pub(crate) fn with_capacity<T>(capacity: usize) -> Result<Vec<T>> {
    let mut v = Vec::new();
    reserve(&mut v, capacity)?;
    Ok(v)
}

/// A fallible copy of a caller slice.
pub(crate) fn copy_of<T: Clone>(items: &[T]) -> Result<Vec<T>> {
    let mut v = with_capacity(items.len())?;
    v.extend_from_slice(items);
    Ok(v)
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
            Self::Callback(id) => write!(formatter, "callback exception {id}"),
            Self::Undefined(what) => write!(formatter, "undefined behaviour: {what}"),
            Self::Unported { upstream, unit } => write!(formatter, "unported {upstream} ({unit})"),
            Self::Stream(error) => write!(formatter, "{error}"),
            Self::ResourceLimit => formatter.write_str("resource limit"),
            Self::Allocation => formatter.write_str("allocation failed"),
        }
    }
}

/// Result of a ported operation.
pub type Result<T> = ::core::result::Result<T, ClipperError>;
