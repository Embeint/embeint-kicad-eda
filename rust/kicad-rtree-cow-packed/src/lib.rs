// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! Pinned COW and packed R-trees. The node, slab, Hilbert and GNU ordering
//! implementations belong to the accepted sibling crates and are reused here.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
mod cow;
mod packed;
pub use cow::{Budget, CowCursor, CowForest, TreeId};
use kicad_rtree::order;
pub use kicad_rtree::{BulkEntry, Error};
pub use kicad_rtree_node::{Coord, NodeData, NodeHandle, RtreeNode};
pub use packed::{Builder, PackedCursor, PackedRtree};
fn reserve<T>(v: &mut Vec<T>, n: usize) -> Result<(), Error> {
    v.try_reserve(n).map_err(|_| Error::ResourceLimit)
}
fn push<T>(v: &mut Vec<T>, x: T) -> Result<(), Error> {
    reserve(v, 1)?;
    v.push(x);
    Ok(())
}
fn uint(x: f64) -> Result<u32, Error> {
    // C++ truncates first; (-1,0) and [UINT_MAX,UINT_MAX+1) are defined too.
    if !x.is_finite() || x <= -1.0 || x >= 4294967296.0 {
        return Err(Error::Conversion);
    }
    Ok(x as u32)
}
fn template<const N: usize, const M: usize>() -> Result<(), Error> {
    if N == 0 || M < 2 || M > i32::MAX as usize / 3 {
        return Err(Error::InvalidTemplate);
    }
    Ok(())
}
