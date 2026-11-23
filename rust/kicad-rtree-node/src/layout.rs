// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference accounting for `SLAB_ALLOCATOR::MemoryUsage`.
//!
//! `MemoryUsage` reports `m_pages.size() * sizeof( PAGE ) + m_freeList.capacity() *
//! sizeof( NODE* )`.  These are the pinned reference's Linux x86-64 (LP64, GCC
//! libstdc++) object sizes, computed from the declared member order and alignment.
//! They describe the C++ objects, not the memory used by this Rust port.  The
//! difftest compares them against `sizeof`/`alignof` reported by the live oracle.

use crate::RtreeError;

/// `sizeof( NODE* )` and `alignof( NODE* )` on Linux x86-64.
pub const POINTER_SIZE: u64 = 8;
/// `sizeof( std::bitset<256> )`: four 64-bit `unsigned long` words in libstdc++.
pub const BITSET_256_SIZE: u64 = 32;
/// `alignof( std::bitset<256> )`.
pub const BITSET_256_ALIGN: u64 = 8;
/// The `alignas( 64 )` on `PAGE::nodes`.
pub const PAGE_NODES_ALIGN: u64 = 64;

/// Size and alignment of a C++ object in the reference ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceLayout {
    pub size: u64,
    pub align: u64,
}

/// Round `value` up to a multiple of the power-of-two `align`.
pub fn align_up(value: u64, align: u64) -> Result<u64, RtreeError> {
    let mask = align.checked_sub(1).ok_or(RtreeError::LayoutOverflow)?;
    value
        .checked_add(mask)
        .map(|sum| sum & !mask)
        .ok_or(RtreeError::LayoutOverflow)
}

/// Append a member of the given layout at the next aligned offset.
pub fn place(offset: u64, member: ReferenceLayout) -> Result<u64, RtreeError> {
    align_up(offset, member.align)?
        .checked_add(member.size)
        .ok_or(RtreeError::LayoutOverflow)
}

/// `sizeof( SLAB_ALLOCATOR<NODE>::PAGE )` and its alignment for a node of the given
/// layout: `alignas( 64 ) NODE nodes[256]` followed by `std::bitset<256> used`.
pub fn page_layout(
    node: ReferenceLayout,
    nodes_per_page: u64,
) -> Result<ReferenceLayout, RtreeError> {
    let nodes = ReferenceLayout {
        size: node
            .size
            .checked_mul(nodes_per_page)
            .ok_or(RtreeError::LayoutOverflow)?,
        align: node.align.max(PAGE_NODES_ALIGN),
    };
    let bitset = ReferenceLayout {
        size: BITSET_256_SIZE,
        align: BITSET_256_ALIGN,
    };
    let end = place(place(0, nodes)?, bitset)?;
    let align = nodes.align.max(bitset.align);
    Ok(ReferenceLayout {
        size: align_up(end, align)?,
        align,
    })
}
