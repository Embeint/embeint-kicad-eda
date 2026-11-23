// SPDX-License-Identifier: GPL-3.0-or-later
use crate::RtreeError;

/// `KIRTREE::HilbertXY2D`.
///
/// The reference starts at `1U << ( aOrder - 1 )`, so only orders `1..=32` are
/// defined.  Unsigned `uint32_t`/`uint64_t` arithmetic is modular in C++ and is
/// reproduced with wrapping operations; coordinates at or above `2^aOrder` are
/// accepted exactly as the reference accepts them.
pub fn hilbert_xy2d(order: i32, x: u32, y: u32) -> Result<u64, RtreeError> {
    if !(1..=32).contains(&order) {
        return Err(RtreeError::OrderOutOfDomain);
    }
    let shift = u32::try_from(order - 1).map_err(|_| RtreeError::OrderOutOfDomain)?;
    let mut d: u64 = 0;
    let mut x = x;
    let mut y = y;
    let mut s: u32 = 1_u32
        .checked_shl(shift)
        .ok_or(RtreeError::OrderOutOfDomain)?;

    while s > 0 {
        let rx: u32 = u32::from(x & s != 0);
        let ry: u32 = u32::from(y & s != 0);

        // d += static_cast<uint64_t>( s ) * s * ( ( 3 * rx ) ^ ry );
        d = d.wrapping_add(
            u64::from(s)
                .wrapping_mul(u64::from(s))
                .wrapping_mul(u64::from(3_u32.wrapping_mul(rx) ^ ry)),
        );

        // Rotate quadrant
        if ry == 0 {
            if rx == 1 {
                x = s.wrapping_mul(2).wrapping_sub(1).wrapping_sub(x);
                y = s.wrapping_mul(2).wrapping_sub(1).wrapping_sub(y);
            }
            core::mem::swap(&mut x, &mut y);
        }
        s >>= 1;
    }
    Ok(d)
}

/// `KIRTREE::HilbertND2D<NUMDIMS>` with `NUMDIMS = coords.len()`.
///
/// Two dimensions delegate to [`hilbert_xy2d`].  Other dimensions take the
/// Morton fallback.  Each signed intermediate and shift is checked where the
/// reference evaluates it, so only genuinely undefined inputs are refused:
///
/// * `bits - 1` in the loop initializer overflows when `aOrder == INT_MIN`;
/// * a coordinate is shifted by `bit + drop`, at most `aOrder - 1`, which is
///   undefined from 32 upwards.  The shift runs only when the loop does, that is
///   when `bits = min( aOrder, 64 / NUMDIMS ) >= 1`.
///
/// So above 64 dimensions (`64 / NUMDIMS == 0`) every order except `INT_MIN`
/// returns 0, and other orders of zero or below also return 0, as in C++.
pub fn hilbert_nd2d(order: i32, coords: &[u32]) -> Result<u64, RtreeError> {
    if let [x, y] = coords {
        return hilbert_xy2d(order, *x, *y);
    }
    let dims = coords.len();
    if dims == 0 {
        return Err(RtreeError::DimensionsOutOfDomain);
    }

    // const int bits = std::min( aOrder, 64 / NUMDIMS ); const int drop = aOrder - bits;
    let per_axis = i32::try_from(64 / dims).map_err(|_| RtreeError::DimensionsOutOfDomain)?;
    let bits = order.min(per_axis);
    let drop = order
        .checked_sub(bits)
        .ok_or(RtreeError::OrderOutOfDomain)?;
    // for( int bit = bits - 1; bit >= 0; --bit ): the initializer itself can overflow.
    let first = bits.checked_sub(1).ok_or(RtreeError::OrderOutOfDomain)?;
    let mut d: u64 = 0;

    for bit in (0..=first).rev() {
        let shift = bit
            .checked_add(drop)
            .and_then(|shift| u32::try_from(shift).ok())
            .ok_or(RtreeError::OrderOutOfDomain)?;
        for coord in coords.iter().rev() {
            d <<= 1;
            d |= u64::from(
                coord
                    .checked_shr(shift)
                    .ok_or(RtreeError::OrderOutOfDomain)?
                    & 1,
            );
        }
    }
    Ok(d)
}

/// Const-generic form of [`hilbert_nd2d`], matching the template signature.
pub fn hilbert_nd<const NUMDIMS: usize>(
    order: i32,
    coords: &[u32; NUMDIMS],
) -> Result<u64, RtreeError> {
    hilbert_nd2d(order, coords)
}
