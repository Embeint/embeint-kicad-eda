// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! Safe port of pinned KiCad `DYNAMIC_RTREE`; see README for the reference ABI,
//! resource and failure lifecycle contracts. Nodes/allocators are the accepted
//! `kicad-rtree-node` types, with no second storage implementation.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod order;
mod traversal;
mod tree;
use core::fmt;
pub use kicad_rtree_node::{Coord, NodeData, RtreeError};
pub use traversal::{SearchRange, TreeIterator};
pub use tree::{BulkEntry, DynamicRtree, Structure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Node(RtreeError),
    /// Signed overflow at the evaluated reference operation.
    Overflow,
    /// Truncating floating-to-integer conversion outside its defined C++ domain.
    FloatConversion,
    /// Caller-configured resource budget; not an assertion of undefined C++ input.
    Resource,
    /// The tree's private representation invariant failed.
    Invariant,
    /// An earlier failed mutation has no defined reference continuation. Clear it
    /// before reuse; size/query methods never expose a partial successful result.
    FailedMutation,
    /// This reference template configuration is not yet supported by this port.
    UnsupportedTemplate,
}
impl From<RtreeError> for Error {
    fn from(value: RtreeError) -> Self {
        Self::Node(value)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(e) => e.fmt(f),
            Self::Overflow => f.write_str("signed arithmetic overflow"),
            Self::FloatConversion => f.write_str("undefined floating integer conversion"),
            Self::Resource => f.write_str("resource budget exceeded"),
            Self::Invariant => f.write_str("tree invariant failure"),
            Self::FailedMutation => f.write_str("clear required after failed mutation"),
            Self::UnsupportedTemplate => f.write_str("unsupported tree template"),
        }
    }
}
impl std::error::Error for Error {}
pub type Result<T> = core::result::Result<T, Error>;
fn add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).ok_or(Error::Overflow)
}
fn sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).ok_or(Error::Overflow)
}
fn mul(a: i64, b: i64) -> Result<i64> {
    a.checked_mul(b).ok_or(Error::Overflow)
}
fn float_i64(v: f64) -> Result<i64> {
    // C++ truncates before checking representability: (-2^63-1, 2^63).
    if !v.is_finite() || v.trunc() < i64::MIN as f64 || v.trunc() >= 9223372036854775808.0 {
        Err(Error::FloatConversion)
    } else {
        Ok(v as i64)
    }
}
