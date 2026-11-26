// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad geometry/distribute.cpp, copyright The KiCad Developers.
//! `GetDeltasForDistributeByGaps` and `GetDeltasForDistributeByPoints` at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Unit `libs.kimath.src.geometry.distribute.application.ui.p001` owns both bodies. Rounding is
//! the accepted `KiROUND<double, int>` with its omitted `aQuiet = false` argument
//! ([`crate::util::ki_round_default`]); its overflow branch calls `kimathLogOverflow`, which stays
//! an explicit deferral to `libs.kimath.src.math.util.application.ui.p001`.
//!
//! Both functions compute in `int`. Every signed `int` operation of the pinned source that can
//! overflow is checked in source order, and the first overflow is reported as
//! [`DistributeError::SignedOverflow`] naming the operation: C++ [expr.pre]/4 leaves such an
//! evaluation undefined, so no result exists for that input. This includes the final
//! `targetPos += end - start` of the gaps loop, whose value is never read but whose evaluation
//! is still undefined on overflow. Inputs with no overflowing operation are never refused.
//!
//! The `double` arithmetic (`totalGap / double( n - 1 )` and `i * perItemGap`) is evaluated
//! exactly as written, one rounded IEEE operation each, without contraction. Both operands of
//! every floating operation are finite, so no NaN selection arises.
use crate::util::ki_round_default;
use crate::VectorDomainError;

pub const UNIT: &str = "libs.kimath.src.geometry.distribute.application.ui.p001";

/// The pinned C++ operation whose signed `int` evaluation would overflow. `index` is the item
/// index `i` of the loop iteration that evaluates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// `aItemExtents.back().first - aItemExtents.front().second`.
    GapsTotalSpace,
    /// `end - start` of an inner item span.
    GapsItemSpan { index: usize },
    /// `totalGap -= end - start`.
    GapsTotalGap { index: usize },
    /// `targetPos - start`.
    GapsTargetOffset { index: usize },
    /// `targetPos - start + KiROUND( accumulatedGaps )`.
    GapsDelta { index: usize },
    /// `targetPos += end - start`.
    GapsTargetAdvance { index: usize },
    /// `aItemPositions.back() - startPos`.
    PointsTotalGaps,
    /// `startPos + KiROUND( i * itemGap )`.
    PointsTarget { index: usize },
    /// `targetPos - aItemPositions[i]`.
    PointsDelta { index: usize },
}

impl core::fmt::Display for Overflow {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::GapsTotalSpace => f.write_str("signed overflow: gaps total space"),
            Self::GapsItemSpan { index } => write!(f, "signed overflow: gaps item span at {index}"),
            Self::GapsTotalGap { index } => write!(f, "signed overflow: gaps total gap at {index}"),
            Self::GapsTargetOffset { index } => {
                write!(f, "signed overflow: gaps target offset at {index}")
            }
            Self::GapsDelta { index } => write!(f, "signed overflow: gaps delta at {index}"),
            Self::GapsTargetAdvance { index } => {
                write!(f, "signed overflow: gaps target advance at {index}")
            }
            Self::PointsTotalGaps => f.write_str("signed overflow: points total gaps"),
            Self::PointsTarget { index } => write!(f, "signed overflow: points target at {index}"),
            Self::PointsDelta { index } => write!(f, "signed overflow: points delta at {index}"),
        }
    }
}

/// Why a distribute call produced no deltas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistributeError {
    /// The input reaches undefined signed `int` overflow in the pinned C++.
    SignedOverflow(Overflow),
    /// `KiROUND` reached its non-quiet overflow branch, whose `kimathLogOverflow` is unported.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
    /// Any other error from the accepted `KiROUND`; it has no such branch for a `double` source.
    Rounding(VectorDomainError),
}

impl From<VectorDomainError> for DistributeError {
    fn from(e: VectorDomainError) -> Self {
        match e {
            VectorDomainError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            other => Self::Rounding(other),
        }
    }
}

type Result<T> = core::result::Result<T, DistributeError>;

fn checked(value: Option<i32>, op: Overflow) -> Result<i32> {
    value.ok_or(DistributeError::SignedOverflow(op))
}

/// `GetDeltasForDistributeByGaps`: given `n` item spans `(start, end)`, the deltas that leave
/// equal gaps between the items. The first and last deltas are 0, and so is every delta for
/// fewer than three items.
pub fn get_deltas_for_distribute_by_gaps(item_extents: &[(i32, i32)]) -> Result<Vec<i32>> {
    let mut deltas = vec![0; item_extents.len()];

    // This only makes sense for 3 or more items
    let (Some(&(_, front_end)), Some(&(back_start, _))) =
        (item_extents.first(), item_extents.last())
    else {
        return Ok(deltas);
    };
    if item_extents.len() < 3 {
        return Ok(deltas);
    }
    let last = item_extents.len() - 1;
    // The inner items, i = 1 .. size() - 2, with their indices.
    let inner = || item_extents.iter().enumerate().take(last).skip(1);

    // The space between the first and last items' inner edges
    let total_space = checked(back_start.checked_sub(front_end), Overflow::GapsTotalSpace)?;
    let mut total_gap = total_space;

    for (i, &(start, end)) in inner() {
        let span = checked(end.checked_sub(start), Overflow::GapsItemSpan { index: i })?;
        total_gap = checked(
            total_gap.checked_sub(span),
            Overflow::GapsTotalGap { index: i },
        )?;
    }

    let per_item_gap = f64::from(total_gap) / (last as f64);

    // Start counting at the end of the first item
    let mut target_pos = front_end;

    // End-cap items don't need to be changed
    for ((i, &(start, end)), delta_out) in inner().zip(deltas.iter_mut().skip(1)) {
        // Take care not to stack rounding errors by keeping the integer accumulator
        // separate and always re-multiplying the gap
        let accumulated_gaps = (i as f64) * per_item_gap;
        let rounded = ki_round_default(accumulated_gaps)?;
        let offset = checked(
            target_pos.checked_sub(start),
            Overflow::GapsTargetOffset { index: i },
        )?;
        let delta = checked(
            offset.checked_add(rounded),
            Overflow::GapsDelta { index: i },
        )?;

        *delta_out = delta;

        // Step over one item span (width or height). The span was already evaluated without
        // overflow in the first loop.
        let span = checked(end.checked_sub(start), Overflow::GapsItemSpan { index: i })?;
        target_pos = checked(
            target_pos.checked_add(span),
            Overflow::GapsTargetAdvance { index: i },
        )?;
    }

    Ok(deltas)
}

/// `GetDeltasForDistributeByPoints`: given `n` positions, the deltas that space them evenly
/// between the first and last. The first and last deltas are 0, and so is every delta for fewer
/// than three items.
pub fn get_deltas_for_distribute_by_points(item_positions: &[i32]) -> Result<Vec<i32>> {
    let mut deltas = vec![0; item_positions.len()];

    // This only makes sense for 3 or more items
    let (Some(&start_pos), Some(&back)) = (item_positions.first(), item_positions.last()) else {
        return Ok(deltas);
    };
    if item_positions.len() < 3 {
        return Ok(deltas);
    }
    let last = item_positions.len() - 1;

    let total_gaps = checked(back.checked_sub(start_pos), Overflow::PointsTotalGaps)?;
    let item_gap = f64::from(total_gaps) / (last as f64);

    // End-cap items don't need to be changed
    let inner = item_positions.iter().enumerate().take(last).skip(1);
    for ((i, &position), delta_out) in inner.zip(deltas.iter_mut().skip(1)) {
        let rounded = ki_round_default((i as f64) * item_gap)?;
        let target_pos = checked(
            start_pos.checked_add(rounded),
            Overflow::PointsTarget { index: i },
        )?;
        let delta = checked(
            target_pos.checked_sub(position),
            Overflow::PointsDelta { index: i },
        )?;

        *delta_out = delta;
    }

    Ok(deltas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fewer_than_three_items_are_unchanged() {
        assert_eq!(get_deltas_for_distribute_by_gaps(&[]), Ok(vec![]));
        assert_eq!(
            get_deltas_for_distribute_by_gaps(&[(i32::MIN, i32::MAX), (i32::MAX, i32::MIN)]),
            Ok(vec![0, 0])
        );
        assert_eq!(get_deltas_for_distribute_by_points(&[]), Ok(vec![]));
        assert_eq!(
            get_deltas_for_distribute_by_points(&[i32::MIN, i32::MAX]),
            Ok(vec![0, 0])
        );
    }

    #[test]
    fn upstream_qa_examples() {
        // qa/tests/libs/kimath/geometry/test_distribute.cpp at the pin.
        assert_eq!(
            get_deltas_for_distribute_by_gaps(&[(0, 100), (200, 300), (400, 500)]),
            Ok(vec![0, 0, 0])
        );
        assert_eq!(
            get_deltas_for_distribute_by_gaps(&[(0, 100), (149, 249), (400, 500)]),
            Ok(vec![0, 51, 0])
        );
        assert_eq!(
            get_deltas_for_distribute_by_gaps(&[(-100, 0), (0, 100), (0, 100), (300, 400)]),
            Ok(vec![0, 33, 167, 0])
        );
        assert_eq!(
            get_deltas_for_distribute_by_points(&[0, 100, 200, 300, 400]),
            Ok(vec![0, 0, 0, 0, 0])
        );
        assert_eq!(
            get_deltas_for_distribute_by_points(&[0, 49, 200]),
            Ok(vec![0, 51, 0])
        );
        assert_eq!(
            get_deltas_for_distribute_by_points(&[0, 0, 0, 100]),
            Ok(vec![0, 33, 67, 0])
        );
    }

    #[test]
    fn rounding_is_half_away_from_zero() {
        // totalGap 1 over two gaps: 0.5 rounds to 1; -1 over two gaps: -0.5 rounds to -1.
        assert_eq!(
            get_deltas_for_distribute_by_gaps(&[(0, 0), (0, 0), (1, 1)]),
            Ok(vec![0, 1, 0])
        );
        assert_eq!(
            get_deltas_for_distribute_by_points(&[0, 0, -1]),
            Ok(vec![0, -1, 0])
        );
    }

    #[test]
    fn signed_overflow_is_refused_at_the_first_operation() {
        assert_eq!(
            get_deltas_for_distribute_by_gaps(&[(0, 1), (0, 0), (i32::MIN, 0)]),
            Err(DistributeError::SignedOverflow(Overflow::GapsTotalSpace))
        );
        assert_eq!(
            get_deltas_for_distribute_by_gaps(&[(0, 0), (i32::MIN, 1), (0, 0)]),
            Err(DistributeError::SignedOverflow(Overflow::GapsItemSpan {
                index: 1
            }))
        );
        // The last advance is never read but its evaluation overflows.
        assert_eq!(
            get_deltas_for_distribute_by_gaps(&[(0, i32::MAX), (0, 1), (i32::MAX, i32::MAX)]),
            Err(DistributeError::SignedOverflow(
                Overflow::GapsTargetAdvance { index: 1 }
            ))
        );
        assert_eq!(
            get_deltas_for_distribute_by_points(&[i32::MIN, 0, 1]),
            Err(DistributeError::SignedOverflow(Overflow::PointsTotalGaps))
        );
        assert_eq!(
            get_deltas_for_distribute_by_points(&[0, i32::MIN, 2]),
            Err(DistributeError::SignedOverflow(Overflow::PointsDelta {
                index: 1
            }))
        );
    }
}
