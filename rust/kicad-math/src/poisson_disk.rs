// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/kimath/src/geometry/poisson_disk.cpp` at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7: `POISSON_DISK::ToroidalUnitTile`, Bridson's
//! Poisson disk sampling on the unit torus, owned by unit
//! `libs.kimath.src.geometry.poisson_disk.application.pure.p001`.
//!
//! The function is ported statement by statement in source order. Its random numbers come from
//! the faithful Boost.Random 1.83 port in `kicad_boost_random` (`mt19937`,
//! `uniform_real_distribution<double>( 0.0, 1.0 )` and `uniform_int_distribution<size_t>`), and
//! its trigonometry from the crate's glibc 2.39 `sincos` port. Local exact comparison and
//! native/wasm replay pass; qualification against the target GCC 13 / Boost 1.83 / glibc 2.39
//! build remains pending hosted CI. Two evaluation-order facts observed in the local compiled
//! object (GCC 15.2, `-O3 -ffp-contract=off`) are reproduced explicitly:
//!
//! - `VECTOR2D p( uniform( rng ), uniform( rng ) )`: GCC evaluates the two constructor
//!   arguments right to left, so the first draw is `p.y` and the second `p.x`.
//! - `std::cos( theta )` and `std::sin( theta )` on the same argument are combined into one
//!   `sincos` call (GCC's sincos pass, active without fast-math). glibc's `sincos` returns the
//!   same bits as its `sin` and `cos`; the port calls `sincos` as the binary does.
//!
//! `theta` is drawn before `rho` (separate statements), and each candidate costs exactly those two
//! draws. `uniform_int_distribution` draws nothing for a single active sample.
//!
//! # Domain and resources
//!
//! `aMinDist <= 0.0 || aMinDist > 0.5` returns an empty vector, as upstream. Every other input
//! reaches `(int) std::ceil( std::sqrt( 2.0 ) / aMinDist )`, a floating-to-integer conversion that
//! C++ defines only when the value fits in `int`. It does not for NaN (both guard comparisons are
//! false) or for `aMinDist` below about `6.59e-10`; [`TileError::UndefinedGridSize`] refuses
//! those before any allocation or draw. Storage is `gridN * gridN` grid cells plus one sample per
//! accepted point, which the caller bounds with [`TileLimits`]; a refusal returns no partial
//! result. Work is bounded by storage: every outer iteration adds a sample or retires an active
//! one, each with at most 30 candidates of 25 cells.

use crate::glibc_trig;
use crate::vector::Vector2D;
use kicad_boost_random::{Mt19937, UniformIntDistribution, UniformRealDistribution};

/// The owning port unit.
pub const UNIT: &str = "libs.kimath.src.geometry.poisson_disk.application.pure.p001";

/// `std::sqrt( 2.0 )`, constant-folded by the compiler to the correctly rounded value.
const SQRT_2: f64 = f64::from_bits(0x3ff6_a09e_667f_3bcd);
/// `M_PI`.
const PI: f64 = f64::from_bits(0x4009_21fb_5444_2d18);
/// `2^31`: the first value `(int)` cannot represent.
const INT_LIMIT: f64 = 2_147_483_648.0;

/// The caller's storage bounds for one tile. Both are at most `i32::MAX`, which keeps every grid
/// and sample index the reference stores in `int` representable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileLimits {
    /// Maximum `gridN * gridN` background cells.
    pub max_grid_cells: u64,
    /// Maximum samples in the result.
    pub max_samples: u64,
}

/// A refused tile. No partial result is returned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TileError {
    /// A limit exceeds `i32::MAX`.
    InvalidLimits(TileLimits),
    /// `std::ceil( std::sqrt( 2.0 ) / aMinDist )` (given) is not representable as `int`, so the
    /// reference's `(int)` conversion is undefined.
    UndefinedGridSize { cells_per_side: f64 },
    /// `gridN * gridN` exceeds the caller's limit.
    GridLimit { grid_n: i32, cells: u64, limit: u64 },
    /// The tile has more than the caller's limit of samples.
    SampleLimit { limit: u64 },
    /// The host could not allocate `count` elements for `what`.
    Allocation { what: &'static str, count: u64 },
    /// An internal index invariant of the algorithm failed. Unreachable: every index is in range
    /// by construction; reported instead of panicking or substituting a value.
    BrokenInvariant,
}

/// `POISSON_DISK::ToroidalUnitTile( double aMinDist, uint32_t aSeed )`, within `limits`.
pub fn toroidal_unit_tile(
    min_dist: f64,
    seed: u32,
    limits: TileLimits,
) -> Result<Vec<Vector2D>, TileError> {
    if limits.max_grid_cells > i32::MAX as u64 || limits.max_samples > i32::MAX as u64 {
        return Err(TileError::InvalidLimits(limits));
    }

    let mut samples: Vec<Vector2D> = Vec::new();

    if min_dist <= 0.0 || min_dist > 0.5 {
        return Ok(samples); // unsupported / degenerate
    }

    // The cell count must tile the torus exactly (see the upstream comment): 1/gridN <=
    // aMinDist/sqrt(2) keeps at most one sample per cell, and 2/gridN >= aMinDist keeps the
    // +-2 scan covering the exclusion radius.
    let cells_per_side = (SQRT_2 / min_dist).ceil();
    if cells_per_side.is_nan() || cells_per_side >= INT_LIMIT {
        return Err(TileError::UndefinedGridSize { cells_per_side });
    }
    // cells_per_side is a positive integer below 2^31 here, so the conversion is exact.
    let grid_n: i32 = 3.max(cells_per_side as i32);
    let cell_size: f64 = 1.0 / f64::from(grid_n);
    let n = i64::from(grid_n);

    let cells = (grid_n as u64) * (grid_n as u64);
    if cells > limits.max_grid_cells {
        return Err(TileError::GridLimit {
            grid_n,
            cells,
            limit: limits.max_grid_cells,
        });
    }
    // cells <= i32::MAX, so it fits usize on every supported target.
    let cell_count = cells as usize;

    let mut grid: Vec<i32> = Vec::new();
    grid.try_reserve_exact(cell_count)
        .map_err(|_| TileError::Allocation {
            what: "grid",
            count: cells,
        })?;
    grid.resize(cell_count, -1);
    let mut active: Vec<i32> = Vec::new();

    // cellOf: both coordinates are finite and in [0, 1] wherever it is called, so the floors
    // are in [0, gridN] and the int conversions are exact.
    let cell_of = |p: Vector2D| -> (i64, i64) {
        let mut gx = (p.x / cell_size).floor() as i64;
        let mut gy = (p.y / cell_size).floor() as i64;
        gx = ((gx % n) + n) % n;
        gy = ((gy % n) + n) % n;
        (gx, gy)
    };

    let tor_dist_sq = |mut dx: f64, mut dy: f64| -> f64 {
        if dx > 0.5 {
            dx -= 1.0;
        }
        if dx < -0.5 {
            dx += 1.0;
        }
        if dy > 0.5 {
            dy -= 1.0;
        }
        if dy < -0.5 {
            dy += 1.0;
        }
        dx * dx + dy * dy
    };

    // Index of cell (gx, gy): (size_t) gy * gridN + gx, within cell_count by construction.
    let cell_index = |gx: i64, gy: i64| -> usize { (gy * n + gx) as usize };

    let mut rng = Mt19937::new(seed);
    let uniform = UniformRealDistribution::new(0.0, 1.0).map_err(|_| TileError::BrokenInvariant)?;

    let push_sample = |samples: &mut Vec<Vector2D>, p: Vector2D| -> Result<(), TileError> {
        if samples.len() as u64 >= limits.max_samples {
            return Err(TileError::SampleLimit {
                limit: limits.max_samples,
            });
        }
        samples.try_reserve(1).map_err(|_| TileError::Allocation {
            what: "samples",
            count: samples.len() as u64 + 1,
        })?;
        samples.push(p);
        Ok(())
    };
    let push_active = |active: &mut Vec<i32>, index: i32| -> Result<(), TileError> {
        active.try_reserve(1).map_err(|_| TileError::Allocation {
            what: "active",
            count: active.len() as u64 + 1,
        })?;
        active.push(index);
        Ok(())
    };
    let set_cell = |grid: &mut Vec<i32>, gx: i64, gy: i64, value: i32| -> Result<(), TileError> {
        let cell = grid
            .get_mut(cell_index(gx, gy))
            .ok_or(TileError::BrokenInvariant)?;
        *cell = value;
        Ok(())
    };

    // Initial sample. GCC evaluates the constructor arguments right to left.
    let first_draw = uniform.sample(&mut rng);
    let second_draw = uniform.sample(&mut rng);
    let p = Vector2D {
        x: second_draw,
        y: first_draw,
    };
    let (gx, gy) = cell_of(p);
    push_sample(&mut samples, p)?;
    set_cell(&mut grid, gx, gy, 0)?;
    push_active(&mut active, 0)?;

    const K: i32 = 30; // candidate attempts per active sample (Bridson default)

    while !active.is_empty() {
        let pick_idx = UniformIntDistribution::new(0, active.len() as u64 - 1)
            .ok_or(TileError::BrokenInvariant)?;
        let idx = pick_idx.sample(&mut rng) as usize;
        let base = active
            .get(idx)
            .and_then(|&sample| samples.get(sample as usize))
            .copied()
            .ok_or(TileError::BrokenInvariant)?;
        let mut accepted = false;

        for _ in 0..K {
            let theta = uniform.sample(&mut rng) * 2.0 * PI;
            let rho = min_dist + uniform.sample(&mut rng) * min_dist; // annulus [r, 2r)
            let (sin_theta, cos_theta) = glibc_trig::sincos(theta);
            let mut cand = Vector2D {
                x: base.x + rho * cos_theta,
                y: base.y + rho * sin_theta,
            };

            // Wrap into the unit square.
            cand.x -= cand.x.floor();
            cand.y -= cand.y.floor();

            let (cx, cy) = cell_of(cand);
            let mut too_close = false;

            // Any sample within aMinDist is within +-2 background cells, modulo toroidal
            // wraparound.
            let mut dy = -2;
            while dy <= 2 && !too_close {
                let ny = ((cy + dy) % n + n) % n;
                let mut dx = -2;
                while dx <= 2 && !too_close {
                    let nx = ((cx + dx) % n + n) % n;
                    dx += 1;
                    let idx_n = *grid
                        .get(cell_index(nx, ny))
                        .ok_or(TileError::BrokenInvariant)?;

                    if idx_n < 0 {
                        continue;
                    }

                    let other = samples
                        .get(idx_n as usize)
                        .ok_or(TileError::BrokenInvariant)?;
                    // aMinDist * aMinDist is evaluated on every comparison, as upstream.
                    if tor_dist_sq(other.x - cand.x, other.y - cand.y) < min_dist * min_dist {
                        too_close = true;
                    }
                }
                dy += 1;
            }

            if too_close {
                continue;
            }

            push_sample(&mut samples, cand)?;
            // samples.len() <= max_samples <= i32::MAX, so (int) samples.size() - 1 is exact.
            let new_index = samples.len() as i32 - 1;
            set_cell(&mut grid, cx, cy, new_index)?;
            push_active(&mut active, new_index)?;
            accepted = true;
            break;
        }

        if !accepted {
            // active[idx] = active.back(); active.pop_back();
            if idx >= active.len() {
                return Err(TileError::BrokenInvariant);
            }
            active.swap_remove(idx);
        }
    }

    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: TileLimits = TileLimits {
        max_grid_cells: 1 << 20,
        max_samples: 1 << 20,
    };

    #[test]
    fn constants_are_the_folded_values() {
        assert_eq!(SQRT_2, 2.0_f64.sqrt());
        assert_eq!(PI, std::f64::consts::PI);
    }

    #[test]
    fn out_of_domain_is_empty() {
        for r in [
            0.0,
            -0.0,
            -1.0,
            0.5000000000000001,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert_eq!(toroidal_unit_tile(r, 1, LIMITS), Ok(Vec::new()));
        }
    }

    #[test]
    fn undefined_conversion_is_refused() {
        assert!(matches!(
            toroidal_unit_tile(f64::NAN, 1, LIMITS),
            Err(TileError::UndefinedGridSize { .. })
        ));
        assert!(matches!(
            toroidal_unit_tile(f64::MIN_POSITIVE, 1, LIMITS),
            Err(TileError::UndefinedGridSize { .. })
        ));
    }

    #[test]
    fn half_distance_tile_is_deterministic_and_spaced() {
        let a = toroidal_unit_tile(0.1, 42, LIMITS);
        let b = toroidal_unit_tile(0.1, 42, LIMITS);
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        let samples = a.unwrap_or_default();
        assert!(samples.len() > 10);
    }
}
