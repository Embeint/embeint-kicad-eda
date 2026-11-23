// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Coord, RtreeError};

struct Fanout<const FANOUT: usize>;

impl<const FANOUT: usize> Fanout<FANOUT> {
    // static_assert( FANOUT <= 31 ) and static_assert( FANOUT % 4 == 0 )
    const VALID: () = assert!(
        FANOUT <= 31 && FANOUT % 4 == 0,
        "OverlapMask2D requires FANOUT <= 31 and divisible by 4"
    );
}

/// `KIRTREE::OverlapMask2D<ELEMTYPE, FANOUT>`, with the template's static
/// assertions checked at compile time.
///
/// Each bounds slice must provide at least `FANOUT` elements: every reference path
/// (SSE2, NEON and scalar) reads all `FANOUT` slots before masking with
/// `( 1U << aCount ) - 1`, so `aCount` must lie in `0..=31`.
pub fn overlap_mask_2d<E: Coord, const FANOUT: usize>(
    min0: &[E],
    max0: &[E],
    min1: &[E],
    max1: &[E],
    count: i32,
    query_min: &[E; 2],
    query_max: &[E; 2],
) -> Result<u32, RtreeError> {
    let () = Fanout::<FANOUT>::VALID;
    overlap_mask_2d_dyn(FANOUT, min0, max0, min1, max1, count, query_min, query_max)
}

/// [`overlap_mask_2d`] with a run-time fanout.  A fanout that would fail the
/// reference's static assertions is refused with [`RtreeError::FanoutOutOfDomain`].
#[allow(clippy::too_many_arguments)]
pub fn overlap_mask_2d_dyn<E: Coord>(
    fanout: usize,
    min0: &[E],
    max0: &[E],
    min1: &[E],
    max1: &[E],
    count: i32,
    query_min: &[E; 2],
    query_max: &[E; 2],
) -> Result<u32, RtreeError> {
    if fanout > 31 || fanout % 4 != 0 {
        return Err(RtreeError::FanoutOutOfDomain);
    }
    if !(0..=31).contains(&count) {
        return Err(RtreeError::CountOutOfDomain);
    }
    fn lane<E>(slice: &[E], fanout: usize) -> Result<&[E], RtreeError> {
        slice.get(..fanout).ok_or(RtreeError::SliceTooShort)
    }
    let (min0, max0) = (lane(min0, fanout)?, lane(max0, fanout)?);
    let (min1, max1) = (lane(min1, fanout)?, lane(max1, fanout)?);

    // The SSE2 path tests "any separation axis" with signed 32-bit compares; for
    // int (and the scalar int64_t path) that is the negation of this predicate.
    let mut mask: u32 = 0;
    for (i, (((lo0, hi0), lo1), hi1)) in min0.iter().zip(max0).zip(min1).zip(max1).enumerate() {
        let overlaps = *lo0 <= query_max[0]
            && *hi0 >= query_min[0]
            && *lo1 <= query_max[1]
            && *hi1 >= query_min[1];
        let shift = u32::try_from(i).map_err(|_| RtreeError::FanoutOutOfDomain)?;
        mask |= u32::from(overlaps) << shift;
    }
    let shift = u32::try_from(count).map_err(|_| RtreeError::CountOutOfDomain)?;
    Ok(mask & (1_u32 << shift).wrapping_sub(1))
}
