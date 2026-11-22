// SPDX-License-Identifier: GPL-3.0-or-later
use crate::layout::{self, ReferenceLayout, POINTER_SIZE};
use crate::{overlap_mask_2d_dyn, Coord, NodeHandle, RtreeError, SlabNode};
use core::fmt::Debug;

/// A leaf `DATATYPE`.
///
/// Every reference instantiation stores a pointer (`VIEW_ITEM*`, `SCH_ITEM*`,
/// `ZONE*`, ...), which shares the `children` union's 8-byte size and alignment.
/// Only such pointer-sized data is ported: it keeps the union's layout, the
/// constructor's zero (null) state and `RemoveChild`'s whole-slot copy exact.
pub trait NodeData: Clone + PartialEq + Debug {}

/// One slot of the reference's `union { RTREE_NODE* children[]; DATATYPE data[]; }`.
#[derive(Clone, Debug, PartialEq)]
pub enum Payload<T> {
    /// All-zero bytes: a null child pointer and a null data pointer, as written by
    /// the constructor's `memset`.
    Null,
    /// The active member is `children[i]`, a non-null node.
    Child(NodeHandle),
    /// The active member is `data[i]`, a non-null item.
    Data(T),
}

/// `KIRTREE::RTREE_NODE<DATATYPE, ELEMTYPE, NUMDIMS, MAXNODES>`.
///
/// The SoA rows `bounds[axis * 2 + side][slot]` are stored as
/// `bounds[axis][side][slot]` because stable Rust cannot size an array by
/// `NUMDIMS * 2`; [`Self::bound`] and friends take the reference row number.
/// `count` and `level` are public as in C++, so every operation that indexes with
/// `count` checks it against the arrays first.  Like the reference (whose
/// `std::atomic` member deletes the copy constructor) a node is not `Clone`.
#[derive(Debug)]
pub struct RtreeNode<T, E, const NUMDIMS: usize, const MAXNODES: usize> {
    /// Number of valid children.
    pub count: i32,
    /// 0 = leaf, higher = internal.
    pub level: i32,
    bounds: [[[E; MAXNODES]; 2]; NUMDIMS],
    payload: [Payload<T>; MAXNODES],
    insert_bounds: [[[E; MAXNODES]; 2]; NUMDIMS],
    // The reference uses std::atomic<int> for copy-on-write sharing.  Portable
    // crates run single-threaded; the host serialises access to an allocator, so
    // the value semantics (two's complement wrap of fetch_add/fetch_sub) suffice.
    refcount: i32,
}

struct Shape<const NUMDIMS: usize, const MAXNODES: usize>;

impl<const NUMDIMS: usize, const MAXNODES: usize> Shape<NUMDIMS, MAXNODES> {
    // Zero-length arrays are ill-formed C++; MAXNODES is an int template argument.
    const VALID: () = assert!(
        NUMDIMS >= 1 && MAXNODES >= 1 && MAXNODES <= i32::MAX as usize,
        "RTREE_NODE requires positive NUMDIMS and MAXNODES"
    );
}

impl<T: NodeData, E: Coord, const NUMDIMS: usize, const MAXNODES: usize> Default
    for RtreeNode<T, E, NUMDIMS, MAXNODES>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T: NodeData, E: Coord, const NUMDIMS: usize, const MAXNODES: usize>
    RtreeNode<T, E, NUMDIMS, MAXNODES>
{
    /// `MAXNODES * 2 / 5`, the R*-tree minimum fill.
    pub const MIN_NODES: i32 = {
        let () = Shape::<NUMDIMS, MAXNODES>::VALID;
        (MAXNODES * 2 / 5) as i32
    };
    const MAX_NODES: i32 = MAXNODES as i32;

    /// `RTREE_NODE()`: zero count, level, bounds, insertion bounds and union;
    /// reference count 1.
    pub fn new() -> Self {
        let () = Shape::<NUMDIMS, MAXNODES>::VALID;
        Self {
            count: 0,
            level: 0,
            bounds: [[[E::default(); MAXNODES]; 2]; NUMDIMS],
            payload: core::array::from_fn(|_| Payload::Null),
            insert_bounds: [[[E::default(); MAXNODES]; 2]; NUMDIMS],
            refcount: 1,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.level == 0
    }

    pub fn is_internal(&self) -> bool {
        self.level > 0
    }

    pub fn is_full(&self) -> bool {
        self.count >= Self::MAX_NODES
    }

    pub fn is_underflow(&self) -> bool {
        self.count < Self::MIN_NODES
    }

    fn slot(i: usize) -> Result<usize, RtreeError> {
        if i < MAXNODES {
            Ok(i)
        } else {
            Err(RtreeError::SlotOutOfRange)
        }
    }

    /// Children visited by loops `for( i = 0; i < count; ++i )`; a negative count
    /// visits none, a count above `MAXNODES` would read past the arrays.
    fn visited(&self) -> Result<usize, RtreeError> {
        if self.count > Self::MAX_NODES {
            return Err(RtreeError::CountOutOfDomain);
        }
        usize::try_from(self.count.max(0)).map_err(|_| RtreeError::CountOutOfDomain)
    }

    fn row(&self, row: usize) -> Result<&[E; MAXNODES], RtreeError> {
        self.bounds
            .get(row / 2)
            .and_then(|axis| axis.get(row % 2))
            .ok_or(RtreeError::SlotOutOfRange)
    }

    /// `bounds[row][i]` with the reference row `axis * 2 + side`.
    pub fn bound(&self, row: usize, i: usize) -> Result<E, RtreeError> {
        Ok(self.row(row)?[Self::slot(i)?])
    }

    /// Assign `bounds[row][i]`.
    pub fn set_bound(&mut self, row: usize, i: usize, value: E) -> Result<(), RtreeError> {
        let i = Self::slot(i)?;
        let cell = self
            .bounds
            .get_mut(row / 2)
            .and_then(|axis| axis.get_mut(row % 2))
            .ok_or(RtreeError::SlotOutOfRange)?;
        cell[i] = value;
        Ok(())
    }

    /// `insertBounds[row][i]` with the reference row `axis * 2 + side`.
    pub fn insert_bound(&self, row: usize, i: usize) -> Result<E, RtreeError> {
        let i = Self::slot(i)?;
        self.insert_bounds
            .get(row / 2)
            .and_then(|axis| axis.get(row % 2))
            .map(|cells| cells[i])
            .ok_or(RtreeError::SlotOutOfRange)
    }

    /// Assign `insertBounds[row][i]`.
    pub fn set_insert_bound(&mut self, row: usize, i: usize, value: E) -> Result<(), RtreeError> {
        let i = Self::slot(i)?;
        let cell = self
            .insert_bounds
            .get_mut(row / 2)
            .and_then(|axis| axis.get_mut(row % 2))
            .ok_or(RtreeError::SlotOutOfRange)?;
        cell[i] = value;
        Ok(())
    }

    /// `ComputeEnclosingBounds`: `max()`/`lowest()` when no child is visited.
    pub fn compute_enclosing_bounds(&self) -> Result<([E; NUMDIMS], [E; NUMDIMS]), RtreeError> {
        let mut min = [E::MAX; NUMDIMS];
        let mut max = [E::LOWEST; NUMDIMS];
        let visited = self.visited()?;

        for i in 0..visited {
            for (d, axis) in self.bounds.iter().enumerate() {
                if axis[0][i] < min[d] {
                    min[d] = axis[0][i];
                }
                if axis[1][i] > max[d] {
                    max[d] = axis[1][i];
                }
            }
        }
        Ok((min, max))
    }

    /// `ChildArea`: the product of `max - min` over the axes, in `int64_t`.
    pub fn child_area(&self, i: usize) -> Result<i64, RtreeError> {
        let i = Self::slot(i)?;
        let mut area: i64 = 1;
        for axis in &self.bounds {
            let extent = axis[1][i]
                .widen()
                .checked_sub(axis[0][i].widen())
                .ok_or(RtreeError::Overflow)?;
            area = area.checked_mul(extent).ok_or(RtreeError::Overflow)?;
        }
        Ok(area)
    }

    /// `ChildOverlaps`: inclusive comparison on every axis.
    pub fn child_overlaps(
        &self,
        i: usize,
        min: &[E; NUMDIMS],
        max: &[E; NUMDIMS],
    ) -> Result<bool, RtreeError> {
        let i = Self::slot(i)?;
        for (d, axis) in self.bounds.iter().enumerate() {
            if axis[0][i] > max[d] || axis[1][i] < min[d] {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `ChildOverlapMask`.
    ///
    /// In two dimensions this is `OverlapMask2D<ELEMTYPE, MAXNODES>` over all
    /// `MAXNODES` slots, masked by `count` (defined for counts `0..=31`, even above
    /// `MAXNODES`); a `MAXNODES` failing its static assertions is refused.  Other
    /// dimensions loop over `count` children and set `1U << i`.
    pub fn child_overlap_mask(
        &self,
        min: &[E; NUMDIMS],
        max: &[E; NUMDIMS],
    ) -> Result<u32, RtreeError> {
        if NUMDIMS == 2 {
            let axis0 = self
                .bounds
                .first()
                .ok_or(RtreeError::DimensionsOutOfDomain)?;
            let axis1 = self
                .bounds
                .get(1)
                .ok_or(RtreeError::DimensionsOutOfDomain)?;
            let pair = |values: &[E; NUMDIMS]| -> Result<[E; 2], RtreeError> {
                match values.as_slice() {
                    [a, b] => Ok([*a, *b]),
                    _ => Err(RtreeError::DimensionsOutOfDomain),
                }
            };
            return overlap_mask_2d_dyn(
                MAXNODES,
                &axis0[0],
                &axis0[1],
                &axis1[0],
                &axis1[1],
                self.count,
                &pair(min)?,
                &pair(max)?,
            );
        }

        let visited = self.visited()?;
        if visited > 32 {
            return Err(RtreeError::CountOutOfDomain);
        }
        let mut mask: u32 = 0;
        for i in 0..visited {
            if self.child_overlaps(i, min, max)? {
                let shift = u32::try_from(i).map_err(|_| RtreeError::CountOutOfDomain)?;
                mask |= 1_u32 << shift;
            }
        }
        Ok(mask)
    }

    /// `ChildOverlapArea`: 0 at the first separated axis, otherwise the product.
    pub fn child_overlap_area(
        &self,
        i: usize,
        min: &[E; NUMDIMS],
        max: &[E; NUMDIMS],
    ) -> Result<i64, RtreeError> {
        let i = Self::slot(i)?;
        let mut overlap: i64 = 1;
        for (d, axis) in self.bounds.iter().enumerate() {
            let lo = axis[0][i].max(min[d]);
            let hi = axis[1][i].min(max[d]);
            if lo > hi {
                return Ok(0);
            }
            let extent = hi
                .widen()
                .checked_sub(lo.widen())
                .ok_or(RtreeError::Overflow)?;
            overlap = overlap.checked_mul(extent).ok_or(RtreeError::Overflow)?;
        }
        Ok(overlap)
    }

    /// `ChildEnlargement`: enlarged area minus `ChildArea( i )`.
    pub fn child_enlargement(
        &self,
        i: usize,
        min: &[E; NUMDIMS],
        max: &[E; NUMDIMS],
    ) -> Result<i64, RtreeError> {
        let original = self.child_area(i)?;
        let i = Self::slot(i)?;
        let mut enlarged: i64 = 1;
        for (d, axis) in self.bounds.iter().enumerate() {
            let lo = axis[0][i].min(min[d]);
            let hi = axis[1][i].max(max[d]);
            let extent = hi
                .widen()
                .checked_sub(lo.widen())
                .ok_or(RtreeError::Overflow)?;
            enlarged = enlarged.checked_mul(extent).ok_or(RtreeError::Overflow)?;
        }
        enlarged.checked_sub(original).ok_or(RtreeError::Overflow)
    }

    /// `ChildPerimeter`: twice the sum of the extents.
    pub fn child_perimeter(&self, i: usize) -> Result<i64, RtreeError> {
        let i = Self::slot(i)?;
        let mut perimeter: i64 = 0;
        for axis in &self.bounds {
            let extent = axis[1][i]
                .widen()
                .checked_sub(axis[0][i].widen())
                .ok_or(RtreeError::Overflow)?;
            perimeter = perimeter.checked_add(extent).ok_or(RtreeError::Overflow)?;
        }
        perimeter.checked_mul(2).ok_or(RtreeError::Overflow)
    }

    /// `SetChildBounds`.
    pub fn set_child_bounds(
        &mut self,
        i: usize,
        min: &[E; NUMDIMS],
        max: &[E; NUMDIMS],
    ) -> Result<(), RtreeError> {
        let i = Self::slot(i)?;
        for (d, axis) in self.bounds.iter_mut().enumerate() {
            axis[0][i] = min[d];
            axis[1][i] = max[d];
        }
        Ok(())
    }

    /// `GetChildBounds`.
    pub fn get_child_bounds(&self, i: usize) -> Result<([E; NUMDIMS], [E; NUMDIMS]), RtreeError> {
        let i = Self::slot(i)?;
        Ok((
            core::array::from_fn(|d| self.bounds[d][0][i]),
            core::array::from_fn(|d| self.bounds[d][1][i]),
        ))
    }

    /// `SetInsertBounds`.
    pub fn set_insert_bounds(
        &mut self,
        i: usize,
        min: &[E; NUMDIMS],
        max: &[E; NUMDIMS],
    ) -> Result<(), RtreeError> {
        let i = Self::slot(i)?;
        for (d, axis) in self.insert_bounds.iter_mut().enumerate() {
            axis[0][i] = min[d];
            axis[1][i] = max[d];
        }
        Ok(())
    }

    /// `GetInsertBounds`.
    pub fn get_insert_bounds(&self, i: usize) -> Result<([E; NUMDIMS], [E; NUMDIMS]), RtreeError> {
        let i = Self::slot(i)?;
        Ok((
            core::array::from_fn(|d| self.insert_bounds[d][0][i]),
            core::array::from_fn(|d| self.insert_bounds[d][1][i]),
        ))
    }

    /// `RemoveChild`: move the last child into slot `i`, then decrement `count`.
    ///
    /// The reference asserts `i < count`; the vacated last slot keeps its bounds,
    /// insertion bounds and pointer.  The leaf branch copies `data[i]` and the
    /// other branch `children[i]`; with pointer-sized `DATATYPE` both copy the
    /// same 8 union bytes, which is the whole-slot copy made here.
    pub fn remove_child(&mut self, i: usize) -> Result<(), RtreeError> {
        let i = Self::slot(i)?;
        if self.count > Self::MAX_NODES {
            return Err(RtreeError::CountOutOfDomain);
        }
        let count = usize::try_from(self.count).map_err(|_| RtreeError::SlotOutOfRange)?;
        if i >= count {
            return Err(RtreeError::SlotOutOfRange);
        }
        let last = count - 1;

        if i != last {
            for axis in self.bounds.iter_mut().chain(self.insert_bounds.iter_mut()) {
                for side in axis.iter_mut() {
                    side[i] = side[last];
                }
            }
            self.payload[i] = self.payload[last].clone();
        }
        self.count -= 1;
        Ok(())
    }

    /// The raw union slot, for inspection and state comparison.
    pub fn payload(&self, i: usize) -> Result<&Payload<T>, RtreeError> {
        Ok(&self.payload[Self::slot(i)?])
    }

    /// `children[i]`: `None` for a null pointer.  Reading a slot whose active
    /// member is `data` is refused.
    pub fn child(&self, i: usize) -> Result<Option<&NodeHandle>, RtreeError> {
        match self.payload(i)? {
            Payload::Null => Ok(None),
            Payload::Child(node) => Ok(Some(node)),
            Payload::Data(_) => Err(RtreeError::InactiveUnionMember),
        }
    }

    /// `data[i]`: `None` for a null pointer.  Reading a slot whose active member
    /// is `children` is refused.
    pub fn data(&self, i: usize) -> Result<Option<&T>, RtreeError> {
        match self.payload(i)? {
            Payload::Null => Ok(None),
            Payload::Data(item) => Ok(Some(item)),
            Payload::Child(_) => Err(RtreeError::InactiveUnionMember),
        }
    }

    /// `children[i] = node` (`None` writes a null pointer).
    pub fn set_child(&mut self, i: usize, node: Option<NodeHandle>) -> Result<(), RtreeError> {
        let i = Self::slot(i)?;
        self.payload[i] = node.map_or(Payload::Null, Payload::Child);
        Ok(())
    }

    /// `data[i] = item` (`None` writes a null pointer).
    pub fn set_data(&mut self, i: usize, item: Option<T>) -> Result<(), RtreeError> {
        let i = Self::slot(i)?;
        self.payload[i] = item.map_or(Payload::Null, Payload::Data);
        Ok(())
    }

    /// `refcount.load()`.
    pub fn refcount_load(&self) -> i32 {
        self.refcount
    }

    /// `refcount.store( value )`.
    pub fn refcount_store(&mut self, value: i32) {
        self.refcount = value;
    }

    /// `refcount.fetch_add( value )`: returns the previous value; signed atomic
    /// arithmetic wraps in C++20.
    pub fn refcount_fetch_add(&mut self, value: i32) -> i32 {
        let previous = self.refcount;
        self.refcount = previous.wrapping_add(value);
        previous
    }

    /// `refcount.fetch_sub( value )`: returns the previous value, wrapping.
    pub fn refcount_fetch_sub(&mut self, value: i32) -> i32 {
        let previous = self.refcount;
        self.refcount = previous.wrapping_sub(value);
        previous
    }
}

impl<T: NodeData, E: Coord, const NUMDIMS: usize, const MAXNODES: usize> SlabNode
    for RtreeNode<T, E, NUMDIMS, MAXNODES>
{
    fn construct() -> Self {
        Self::new()
    }

    /// `int count, level; ELEMTYPE bounds[NUMDIMS * 2][MAXNODES]; union { pointers };
    /// ELEMTYPE insertBounds[NUMDIMS * 2][MAXNODES]; std::atomic<int> refcount;`
    fn reference_layout() -> Result<ReferenceLayout, RtreeError> {
        let overflow = RtreeError::LayoutOverflow;
        let slots = u64::try_from(MAXNODES).map_err(|_| overflow)?;
        let rows = u64::try_from(NUMDIMS)
            .ok()
            .and_then(|dims| dims.checked_mul(2))
            .ok_or(overflow)?;
        let int = ReferenceLayout { size: 4, align: 4 };
        let soa = ReferenceLayout {
            size: rows
                .checked_mul(slots)
                .and_then(|cells| cells.checked_mul(E::REFERENCE_SIZE))
                .ok_or(overflow)?,
            align: E::REFERENCE_ALIGN,
        };
        let union = ReferenceLayout {
            size: slots.checked_mul(POINTER_SIZE).ok_or(overflow)?,
            align: POINTER_SIZE,
        };
        let mut offset = layout::place(0, int)?;
        offset = layout::place(offset, int)?;
        offset = layout::place(offset, soa)?;
        offset = layout::place(offset, union)?;
        offset = layout::place(offset, soa)?;
        offset = layout::place(offset, int)?;
        let align = int.align.max(soa.align).max(union.align);
        Ok(ReferenceLayout {
            size: layout::align_up(offset, align)?,
            align,
        })
    }
}
