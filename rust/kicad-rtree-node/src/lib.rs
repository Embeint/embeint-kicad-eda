// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! Behaviour of `libs/kimath/include/geometry/rtree/rtree_node.h` at KiCad pin
//! `4cf939778df04d2b747bb2c4714fc5014a7565c7`: `HilbertXY2D`, `HilbertND2D`,
//! `OverlapMask2D`, `RTREE_NODE` and `SLAB_ALLOCATOR`.
//!
//! Inputs outside the reference's defined domain (shifts by the type width, signed
//! overflow, out-of-bounds slots, use after free, nodes from another allocator) are
//! refused with a typed [`RtreeError`] instead of being given an invented result.
//! See `README.md` for the storage design and the unported branches.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod coord;
mod error;
mod hilbert;
pub mod layout;
mod node;
mod overlap;
mod slab;

pub use coord::Coord;
pub use error::RtreeError;
pub use hilbert::{hilbert_nd, hilbert_nd2d, hilbert_xy2d};
pub use node::{NodeData, Payload, RtreeNode};
pub use overlap::{overlap_mask_2d, overlap_mask_2d_dyn};
pub use slab::{NodeHandle, SlabAllocator, SlabNode, NODES_PER_PAGE};

/// Every definition of `rtree_node.h`, under the header's name (the unit record's
/// `rust_module`).
pub mod rtree_node {
    pub use crate::layout;
    pub use crate::{
        hilbert_nd, hilbert_nd2d, hilbert_xy2d, overlap_mask_2d, overlap_mask_2d_dyn, Coord,
        NodeData, NodeHandle, Payload, RtreeError, RtreeNode, SlabAllocator, SlabNode,
        NODES_PER_PAGE,
    };
}
