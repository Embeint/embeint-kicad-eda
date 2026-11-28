// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/fracture_edge_index_utils.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `KIGEOM::FRACTURE_INDEX` sizing helpers (libs/kimath/include/geometry/
//! fracture_edge_index_utils.h) at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! `size_t` is the reference platform's 64-bit `unsigned long`, so it is `u64` here on every
//! target (wasm32 included), never `usize`. Unsigned arithmetic wraps exactly as the C++ does
//! where the header lets it (`aStripeCount + 1`, `( MAX_BUCKET_SPAN + 2 ) * aHoleCount`,
//! `aStripeCount - 1`); every other operation provably stays in range. Every body is defined
//! for every input.

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.pure.p001";

pub const MAX_STRIPES: u32 = 65536;
pub const MAX_BUCKET_SPAN: u32 = 8;
/// The index is at most ~1.6x the edge set it serves, so a budget proportional to the edge set
/// bounds the footprint without dropping the index on the large planes that need it most.
pub const EDGE_SET_BUDGET_MULTIPLE: u64 = 2;
pub const MIN_EDGE_VISITS: u64 = 32768;
pub const MIN_HOLE_COUNT: u64 = 8;

/// `sizeof( uint32_t )`.
const U32_SIZE: u64 = 4;

/// `StripeCountFor`: `sqrt( aEdgeCount )` clamped to `[1, MAX_STRIPES]`, truncated.
pub fn stripe_count_for(edge_count: u64) -> u32 {
    // Conversion to double rounds to nearest; sqrt is correctly rounded on every target.
    let root = (edge_count as f64).sqrt();
    let count = crate::util::min(crate::util::max(root, 1.0), f64::from(MAX_STRIPES));
    // count is in [1, 65536], so the truncation is in range.
    count as u32
}

/// `MapYToStripe`.
pub fn map_y_to_stripe(y: i32, min_y: i32, max_y: i32, stripe_count: u32) -> u32 {
    // An inverted extent would divide by a non-positive range and violate the clamp bounds
    if max_y < min_y {
        return 0;
    }

    // range is in [1, 2^32] and offset in [0, range - 1], so every operation is in range and
    // the product is below 2^64.
    let range = i64::from(max_y) - i64::from(min_y) + 1;
    let offset = (i64::from(y) - i64::from(min_y)).clamp(0, range - 1);
    let mapped = offset as u64 * u64::from(stripe_count) / range as u64;
    // static_cast<uint32_t>( mapped ) keeps the low 32 bits; aStripeCount - 1 wraps for 0.
    crate::util::min(mapped as u32, stripe_count.wrapping_sub(1))
}

/// `StripeSpan`: the stripes of the lower and the upper of the two values.
pub fn stripe_span(y1: i32, y2: i32, min_y: i32, max_y: i32, stripe_count: u32) -> (u32, u32) {
    (
        map_y_to_stripe(crate::util::min(y1, y2), min_y, max_y, stripe_count),
        map_y_to_stripe(crate::util::max(y1, y2), min_y, max_y, stripe_count),
    )
}

/// `CheckedAdd`: adds `aCount * aElementSize` to `aTotal` unless that overflows `size_t`.
pub fn checked_add(total: &mut u64, count: u64, element_size: u64) -> bool {
    if element_size == 0 {
        return true;
    }

    // total <= u64::MAX, so the subtraction does not wrap; element_size > 0.
    if count > (u64::MAX - *total) / element_size {
        return false;
    }

    // Both in range after the test above.
    *total += count * element_size;
    true
}

/// `CapacityBudget`.
pub fn capacity_budget(edge_count: u64, edge_size: u64) -> u64 {
    let mut budget = 0;

    for _ in 0..EDGE_SET_BUDGET_MULTIPLE {
        if !checked_add(&mut budget, edge_count, edge_size) {
            return u64::MAX;
        }
    }

    budget
}

/// `CapacityFits`: the conjunction short-circuits left to right.
pub fn capacity_fits(
    bucket_ids: u64,
    long_ids: u64,
    stripe_count: u64,
    hole_count: u64,
    node_size: u64,
    budget: u64,
) -> bool {
    let mut bytes = 0;

    checked_add(&mut bytes, bucket_ids, U32_SIZE)
        && checked_add(&mut bytes, long_ids, U32_SIZE)
        && checked_add(&mut bytes, stripe_count.wrapping_add(1), U32_SIZE)
        && checked_add(&mut bytes, stripe_count, U32_SIZE)
        && checked_add(&mut bytes, stripe_count.wrapping_add(1), U32_SIZE)
        && checked_add(
            &mut bytes,
            u64::from(MAX_BUCKET_SPAN + 2).wrapping_mul(hole_count),
            node_size,
        )
        && bytes <= budget
}

/// `ActualCapacityFits( ..., size_t* aBytes = nullptr )`: `bytes` receives the total when every
/// addition fits, otherwise 0.
#[allow(clippy::too_many_arguments)]
pub fn actual_capacity_fits(
    bucket_ids: u64,
    long_ids: u64,
    offsets: u64,
    scratch: u64,
    heads: u64,
    nodes: u64,
    node_size: u64,
    budget: u64,
    bytes_out: Option<&mut u64>,
) -> bool {
    let mut bytes = 0;

    let valid = checked_add(&mut bytes, bucket_ids, U32_SIZE)
        && checked_add(&mut bytes, long_ids, U32_SIZE)
        && checked_add(&mut bytes, offsets, U32_SIZE)
        && checked_add(&mut bytes, scratch, U32_SIZE)
        && checked_add(&mut bytes, heads, U32_SIZE)
        && checked_add(&mut bytes, nodes, node_size);

    if let Some(out) = bytes_out {
        *out = if valid { bytes } else { 0 };
    }

    valid && bytes <= budget
}

/// `ShouldIndex`.
pub fn should_index(estimated_visits: u64, hole_count: u64) -> bool {
    hole_count >= MIN_HOLE_COUNT && estimated_visits >= MIN_EDGE_VISITS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripes_and_capacity() {
        assert_eq!(stripe_count_for(0), 1);
        assert_eq!(stripe_count_for(99), 9);
        assert_eq!(stripe_count_for(u64::MAX), MAX_STRIPES);
        assert_eq!(map_y_to_stripe(5, 10, 0, 4), 0);
        assert_eq!(
            map_y_to_stripe(i32::MAX, i32::MIN, i32::MAX, u32::MAX),
            u32::MAX - 1
        );
        assert_eq!(map_y_to_stripe(3, 0, 9, 0), 0);
        assert_eq!(stripe_span(9, 0, 0, 9, 10), (0, 9));
        let mut total = u64::MAX - 3;
        assert!(!checked_add(&mut total, 1, 4));
        assert!(checked_add(&mut total, 1, 3));
        assert_eq!(total, u64::MAX);
        assert_eq!(capacity_budget(u64::MAX / 2, 1), u64::MAX - 1);
        assert_eq!(capacity_budget(u64::MAX / 2 + 1, 1), u64::MAX);
        // 4 + 4 + 8 + 4 + 8 + 10 * 1 bytes.
        assert!(capacity_fits(1, 1, 1, 1, 1, 38));
        assert!(!capacity_fits(1, 1, 1, 1, 1, 37));
        // stripe_count + 1 wraps to 0, then stripe_count * 4 overflows.
        assert!(!capacity_fits(0, 0, u64::MAX, 0, 1, u64::MAX));
        let mut bytes = 7;
        assert!(!actual_capacity_fits(
            u64::MAX,
            0,
            0,
            0,
            0,
            0,
            0,
            u64::MAX,
            Some(&mut bytes)
        ));
        assert_eq!(bytes, 0);
        assert!(should_index(MIN_EDGE_VISITS, MIN_HOLE_COUNT));
    }
}
