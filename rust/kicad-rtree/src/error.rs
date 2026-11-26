// SPDX-License-Identifier: GPL-3.0-or-later
use core::fmt;
use kicad_rtree_node::RtreeError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Node(RtreeError),
    /// The exact signed intermediate evaluated by C++ is undefined.
    Overflow,
    /// Floating-to-integer conversion is outside the C++ destination domain.
    Conversion,
    /// Explicit host resource budget, distinct from undefined reference input.
    ResourceLimit,
    /// The tree's explicit active-node storage budget would be exceeded.
    /// Like `ResourceLimit` this is a host choice, not undefined reference input.
    NodeBudget,
    InvalidTemplate,
    /// A mutation refused undefined arithmetic after beginning to change nodes.
    /// Clear the tree before reusing it; C++ defines no postcondition for that call.
    InterruptedMutation,
    Invariant,
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
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
            Self::Conversion => f.write_str("floating conversion outside reference domain"),
            Self::ResourceLimit => f.write_str("resource limit"),
            Self::NodeBudget => f.write_str("node budget"),
            Self::InvalidTemplate => f.write_str("invalid template parameters"),
            Self::InterruptedMutation => {
                f.write_str("tree requires clear after interrupted mutation")
            }
            Self::Invariant => f.write_str("tree invariant violation"),
            Self::Unported { upstream, unit } => write!(f, "unported {upstream} ({unit})"),
        }
    }
}
impl std::error::Error for Error {}

pub(crate) fn add(a: i64, b: i64) -> Result<i64, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}
pub(crate) fn sub(a: i64, b: i64) -> Result<i64, Error> {
    a.checked_sub(b).ok_or(Error::Overflow)
}
pub(crate) fn mul(a: i64, b: i64) -> Result<i64, Error> {
    a.checked_mul(b).ok_or(Error::Overflow)
}
pub(crate) fn reserve<T>(v: &mut Vec<T>, count: usize) -> Result<(), Error> {
    v.try_reserve(count).map_err(|_| Error::ResourceLimit)
}
pub(crate) fn push<T>(v: &mut Vec<T>, value: T) -> Result<(), Error> {
    reserve(v, 1)?;
    v.push(value);
    Ok(())
}
