// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2016 Mark Roszko <mark.roszko@gmail.com>
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
#![no_std]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::indexing_slicing
)]

//! Portable port of KiCad's `libs/sexpr` at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Two units are ported here:
//!
//! - `libs.sexpr.include.sexpr.application.pure.p001` ([`header`]): `sexpr.h`,
//!   `isexprable.h`, `sexpr_exception.h` and `sexpr_parser.h`. The tree types and their inline
//!   predicates and constructors, the `AsSymbol`/`AsString` string wrappers, the scan and
//!   children argument types, the `SEXPR_LIST` constructors and its `Scan`/`AddChildren`
//!   templates, `ISEXPRABLE` and the two exception classes. `sexpr_parser.h` only declares
//!   `SEXPR::PARSER`; its bodies (`sexpr_parser.cpp`, including the file-reading entry points)
//!   belong to `libs.sexpr.application.ui.p001` and are not part of this crate.
//! - `libs.sexpr.application.pure.p001` ([`sexpr_cpp`]): the out-of-line bodies of
//!   `sexpr.cpp`.
//!
//! Representation choices, each preserving the reference's observable values:
//!
//! - `std::string` is a byte string (`Vec<u8>`/`&[u8]`), never assumed to be UTF-8.
//! - `size_t` (line numbers, child counts and indices, the `AsString` level) is `u64`, the
//!   width of the LP64 reference, on every target including wasm32.
//! - A `SEXPR*` child is `Option<Box<Sexpr>>`: the reference allows null children (for example
//!   from `ISEXPRABLE::SerializeSEXPR`), and the list owns and deletes the others. Shared or
//!   self-referencing children, which the reference deletes twice, are not representable.
//! - A C++ exception thrown by the library becomes a typed [`SexprError`] carrying the same
//!   observable data (`what()`); an operation whose pinned evaluation is undefined (an
//!   out-of-range `operator[]`, a null child dereference, a `double` written through an
//!   `int32_t*`) returns [`SexprError::Undefined`] instead.

extern crate alloc;

pub mod header;
pub mod sexpr_cpp;

pub use header::{
    as_string, as_string_in, as_symbol, as_symbol_in, ChildrenArg, ChildrenArgType,
    DefaultIsexprable, InString, InvalidTypeException, Isexprable, Node, OutString, ParseException,
    ScanArg, ScanArgType, Sexpr, SexprDouble, SexprInteger, SexprList, SexprString, SexprSymbol,
    SexprType, SexprVector, StdException, View, ViewMut,
};
pub use sexpr_cpp::SexprOps;

/// The header unit's identifier.
pub const HEADER_UNIT: &str = "libs.sexpr.include.sexpr.application.pure.p001";
/// The source unit's identifier.
pub const SOURCE_UNIT: &str = "libs.sexpr.application.pure.p001";

/// What a `libs/sexpr` operation can raise instead of returning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SexprError {
    /// `SEXPR::INVALID_TYPE_EXCEPTION`, thrown by the typed accessors.
    InvalidType(InvalidTypeException),
    /// `std::invalid_argument` with this `what()`, thrown by the stream extraction operators.
    InvalidArgument(&'static str),
    /// `std::length_error` with this `what()`, thrown by `std::string` when a requested length
    /// exceeds `max_size()`.
    LengthError(&'static str),
    /// `std::bad_alloc`: an allocation the reference attempts failed.
    BadAlloc,
    /// The pinned C++ evaluation is undefined for this input (no reference result exists).
    Undefined,
    /// The operation reaches a function of a unit that is not ported yet.
    Unported {
        /// The C++ symbol that is not available.
        upstream: &'static str,
        /// The unit that owns it.
        unit: &'static str,
    },
}

impl From<InvalidTypeException> for SexprError {
    fn from(exception: InvalidTypeException) -> Self {
        Self::InvalidType(exception)
    }
}
