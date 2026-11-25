// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad arc_chord_params.h / arc_chord_params.cpp, copyright The KiCad Developers.
//! `ARC_CHORD_PARAMS` at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Unit `libs.kimath.src.geometry.arc_chord_params.application.pure.p001` owns the out-of-line
//! bodies from `geometry/arc_chord_params.cpp`: `Compute`, `GetCenterPoint`, `GetStartAngle`,
//! `GetEndAngle` and `GetArcAngle`. The class layout, its default member initializers and the
//! trivial inline accessors belong to `geometry/arc_chord_params.h`, owned by
//! `libs.kimath.include.geometry.application.pure.p001`; they are co-ported here as the minimum
//! type needed to hold and observe the state (plain field reads, no other behaviour).
//!
//! Arithmetic follows the source expression order without contraction. Binary operations use
//! the accepted reference NaN selection (`fp` and the angle division), so the NaN results that a
//! default-constructed or partially updated object produces are the same natively and in wasm.
use crate::eda_angle::{div, EdaAngle, GeometryResult};
use crate::{fp, glibc_libm, Vector2, Vector2D, Vector2I};

pub const UNIT: &str = "libs.kimath.src.geometry.arc_chord_params.application.pure.p001";
pub const HEADER_UNIT: &str = "libs.kimath.include.geometry.application.pure.p001";

/// Arc geometry in a chord-based coordinate system: `u` along the chord from start to end and
/// `n` perpendicular to it, toward the arc centre. Fields keep their values across a failed
/// `compute`, exactly as the C++ members do: only the fields assigned before the early return
/// change.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArcChordParams {
    chord_len: f64,
    half_chord: f64,
    ux: f64,
    uy: f64,
    nx: f64,
    ny: f64,
    sagitta: f64,
    radius: f64,
    center_offset: f64,
    midx: f64,
    midy: f64,
    valid: bool,
}

impl ArcChordParams {
    /// `ARC_CHORD_PARAMS::Compute`: computes the arc through `start`, `mid` (a point on the arc)
    /// and `end`. Returns false for a zero-length chord or collinear points.
    ///
    /// For 32-bit integer inputs the `sagitta <= 0` and `radius <= 0` refusals cannot be
    /// reached (a nonzero cross product of integer-valued doubles is at least 1), but they are
    /// kept in source order.
    pub fn compute(&mut self, start: &Vector2I, mid: &Vector2I, end: &Vector2I) -> bool {
        self.valid = false;

        let dx = fp::sub(f64::from(end.x), f64::from(start.x));
        let dy = fp::sub(f64::from(end.y), f64::from(start.y));
        // The radicand is a finite non-negative sum of squares of at most 2^33 in magnitude, so
        // the correctly rounded square root is the same on every target (no NaN arises).
        self.chord_len = fp::add(fp::mul(dx, dx), fp::mul(dy, dy)).sqrt();

        if self.chord_len <= 0.0 {
            return false;
        }

        let mx = fp::sub(f64::from(mid.x), f64::from(start.x));
        let my = fp::sub(f64::from(mid.y), f64::from(start.y));
        let cross = fp::sub(fp::mul(mx, dy), fp::mul(my, dx));

        if cross == 0.0 {
            return false;
        }

        self.sagitta = div(cross.abs(), self.chord_len);

        if self.sagitta <= 0.0 {
            return false;
        }

        self.half_chord = div(self.chord_len, 2.0);
        self.radius = div(
            fp::add(
                fp::mul(self.half_chord, self.half_chord),
                fp::mul(self.sagitta, self.sagitta),
            ),
            fp::mul(2.0, self.sagitta),
        );

        if self.radius <= 0.0 {
            return false;
        }

        self.ux = div(dx, self.chord_len);
        self.uy = div(dy, self.chord_len);
        self.nx = -self.uy;
        self.ny = self.ux;

        if cross < 0.0 {
            self.nx = -self.nx;
            self.ny = -self.ny;
        }

        self.center_offset = fp::sub(self.radius, self.sagitta);
        self.midx = fp::mul(fp::add(f64::from(start.x), f64::from(end.x)), 0.5);
        self.midy = fp::mul(fp::add(f64::from(start.y), f64::from(end.y)), 0.5);

        self.valid = true;
        true
    }

    /// `ARC_CHORD_PARAMS::GetCenterPoint`.
    pub fn get_center_point(&self) -> Vector2D {
        // Center is at chord midpoint + center_offset along the normal direction
        Vector2::new(
            fp::add(self.midx, fp::mul(self.nx, self.center_offset)),
            fp::add(self.midy, fp::mul(self.ny, self.center_offset)),
        )
    }

    /// `ARC_CHORD_PARAMS::GetStartAngle`.
    pub fn get_start_angle(&self) -> GeometryResult<EdaAngle> {
        let sin_half = div(self.half_chord, self.radius);
        let cos_half = div(self.center_offset, self.radius);

        // Direction from center to start: -sin(half_angle) * u - cos(half_angle) * n
        EdaAngle::from_vector(Vector2::new(
            fp::sub(fp::mul(-sin_half, self.ux), fp::mul(cos_half, self.nx)),
            fp::sub(fp::mul(-sin_half, self.uy), fp::mul(cos_half, self.ny)),
        ))
    }

    /// `ARC_CHORD_PARAMS::GetEndAngle`.
    pub fn get_end_angle(&self) -> GeometryResult<EdaAngle> {
        let sin_half = div(self.half_chord, self.radius);
        let cos_half = div(self.center_offset, self.radius);

        // Direction from center to end: sin(half_angle) * u - cos(half_angle) * n
        EdaAngle::from_vector(Vector2::new(
            fp::sub(fp::mul(sin_half, self.ux), fp::mul(cos_half, self.nx)),
            fp::sub(fp::mul(sin_half, self.uy), fp::mul(cos_half, self.ny)),
        ))
    }

    /// `ARC_CHORD_PARAMS::GetArcAngle`: the swept angle in radians.
    pub fn get_arc_angle(&self) -> f64 {
        let ratio = clamp(div(self.half_chord, self.radius), 0.0, 1.0);
        let base_angle = fp::mul(2.0, glibc_libm::asin(ratio));

        if self.sagitta > self.radius {
            return fp::sub(2.0 * core::f64::consts::PI, base_angle);
        }

        base_angle
    }

    // Header-owned inline accessors (minimum co-port of arc_chord_params.h).
    pub fn is_valid(&self) -> bool {
        self.valid
    }
    pub fn get_chord_length(&self) -> f64 {
        self.chord_len
    }
    pub fn get_half_chord(&self) -> f64 {
        self.half_chord
    }
    pub fn get_chord_unit_vec(&self) -> Vector2D {
        Vector2::new(self.ux, self.uy)
    }
    pub fn get_normal_unit_vec(&self) -> Vector2D {
        Vector2::new(self.nx, self.ny)
    }
    pub fn get_sagitta(&self) -> f64 {
        self.sagitta
    }
    pub fn get_radius(&self) -> f64 {
        self.radius
    }
    pub fn get_center_offset(&self) -> f64 {
        self.center_offset
    }
    pub fn get_chord_midpoint(&self) -> Vector2D {
        Vector2::new(self.midx, self.midy)
    }
    pub fn get_ux(&self) -> f64 {
        self.ux
    }
    pub fn get_uy(&self) -> f64 {
        self.uy
    }
    pub fn get_nx(&self) -> f64 {
        self.nx
    }
    pub fn get_ny(&self) -> f64 {
        self.ny
    }
    pub fn get_mid_x(&self) -> f64 {
        self.midx
    }
    pub fn get_mid_y(&self) -> f64 {
        self.midy
    }
}

/// `std::clamp( v, lo, hi )` for `double` with lo <= hi, as libstdc++ evaluates it
/// (`min( max( v, lo ), hi )`, each a single `<`): a NaN or -0 `v` passes through unchanged.
fn clamp(value: f64, lo: f64, hi: f64) -> f64 {
    let raised = if value < lo { lo } else { value };
    if hi < raised {
        hi
    } else {
        raised
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arc(points: [(i32, i32); 3]) -> (bool, ArcChordParams) {
        let [s, m, e] = points.map(|(x, y)| Vector2::new(x, y));
        let mut params = ArcChordParams::default();
        (params.compute(&s, &m, &e), params)
    }

    #[test]
    fn degenerate_inputs_are_refused_and_keep_stale_fields() {
        let (ok, mut params) = arc([(0, 0), (5, 5), (10, 0)]);
        assert!(ok && params.is_valid());
        let before = params;
        // Zero chord: only the chord length and validity change.
        assert!(!params.compute(
            &Vector2::new(3, 3),
            &Vector2::new(1, 1),
            &Vector2::new(3, 3)
        ));
        assert!(!params.is_valid());
        assert_eq!(params.get_chord_length(), 0.0);
        assert_eq!(params.get_radius(), before.get_radius());
        // Collinear: chord length updated, the rest stale.
        assert!(!params.compute(
            &Vector2::new(0, 0),
            &Vector2::new(2, 2),
            &Vector2::new(4, 4)
        ));
        assert_eq!(params.get_chord_length(), 32.0_f64.sqrt());
        assert_eq!(params.get_sagitta(), before.get_sagitta());
    }

    #[test]
    fn semicircle_and_reflex_arcs() -> GeometryResult<()> {
        let (ok, params) = arc([(-10, 0), (0, 10), (10, 0)]);
        assert!(ok);
        assert_eq!(params.get_radius(), 10.0);
        assert_eq!(params.get_center_offset(), 0.0);
        assert_eq!(params.get_arc_angle(), core::f64::consts::PI);
        assert_eq!(params.get_start_angle()?.as_degrees(), -180.0);
        assert_eq!(params.get_end_angle()?.as_degrees(), 0.0);
        let center = params.get_center_point();
        assert_eq!((center.x, center.y), (0.0, 0.0));
        let (ok, reflex) = arc([(-10, 0), (0, -30), (10, 0)]);
        assert!(ok && reflex.get_sagitta() > reflex.get_radius());
        assert!(reflex.get_arc_angle() > core::f64::consts::PI);
        Ok(())
    }

    #[test]
    fn default_state_produces_the_reference_nans() -> GeometryResult<()> {
        let params = ArcChordParams::default();
        let center = params.get_center_point();
        assert_eq!((center.x.to_bits(), center.y.to_bits()), (0, 0));
        assert_eq!(params.get_arc_angle().to_bits(), 0xfff8_0000_0000_0000);
        assert!(params.get_start_angle()?.as_degrees().is_nan());
        assert_eq!(clamp(-0.0, 0.0, 1.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(clamp(1.5, 0.0, 1.0), 1.0);
        Ok(())
    }
}
