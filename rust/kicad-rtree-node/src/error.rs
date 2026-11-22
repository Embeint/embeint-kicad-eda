// SPDX-License-Identifier: GPL-3.0-or-later
use core::fmt;

/// Refusals for inputs where the C++ reference has no defined result, or for
/// reference branches that this safe port deliberately does not reproduce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtreeError {
    /// A Hilbert order whose shift would be by a negative amount or at least the
    /// width of `uint32_t` (undefined in C++).
    OrderOutOfDomain,
    /// `HilbertND2D<0>` does not compile (`64 / NUMDIMS`).
    DimensionsOutOfDomain,
    /// `OverlapMask2D` fails its static assertions (`FANOUT <= 31`, `FANOUT % 4 == 0`).
    FanoutOutOfDomain,
    /// A child count that would index past the node arrays or shift by 32 or more.
    CountOutOfDomain,
    /// A child slot outside `0..MAXNODES`, or outside `0..count` for `RemoveChild`.
    SlotOutOfRange,
    /// A bounds slice shorter than the `FANOUT` elements `OverlapMask2D` reads.
    SliceTooShort,
    /// Signed 64-bit overflow in an area, perimeter or enlargement (undefined in C++).
    Overflow,
    /// Reading the inactive member of the `children`/`data` union.
    InactiveUnionMember,
    /// A node that no page of this allocator contains.  The reference would destroy
    /// and later re-construct an object at that foreign address; that is not ported.
    ForeignNode,
    /// A node whose slot is not currently allocated (use after free or double free).
    NodeNotLive,
    /// Two handles to the same node where distinct mutable nodes are required.
    AliasedNodes,
    /// The reference layout computation does not fit in 64 bits.
    LayoutOverflow,
}

impl fmt::Display for RtreeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OrderOutOfDomain => "order outside reference domain",
            Self::DimensionsOutOfDomain => "dimensions outside reference domain",
            Self::FanoutOutOfDomain => "fanout outside reference domain",
            Self::CountOutOfDomain => "count outside reference domain",
            Self::SlotOutOfRange => "slot outside node",
            Self::SliceTooShort => "bounds shorter than fanout",
            Self::Overflow => "arithmetic overflow outside reference domain",
            Self::InactiveUnionMember => "inactive union member",
            Self::ForeignNode => "node is not owned",
            Self::NodeNotLive => "node is not live",
            Self::AliasedNodes => "aliased nodes",
            Self::LayoutOverflow => "layout overflow",
        })
    }
}

impl std::error::Error for RtreeError {}
