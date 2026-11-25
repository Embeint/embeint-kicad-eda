// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad eda_angle.cpp, copyright The KiCad Developers.
//! Out-of-line `EDA_ANGLE` bodies from `libs/kimath/src/geometry/eda_angle.cpp` at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by unit
//! `libs.kimath.src.geometry.eda_angle.application.pure.p001`.
//!
//! They extend the canonical [`EdaAngle`] of the header unit
//! (`libs.kimath.include.geometry.application.ui.p001`) and reuse its `Normalize` and
//! `foldToTwoPeriods` ports. Every operation is total on `double`: non-finite values take the
//! source's explicit early returns, and the folded loops are bounded because
//! `foldToTwoPeriods` leaves a finite value within two periods of zero.
use crate::eda_angle::{AngleUnit, EdaAngle};
use crate::fp;

pub const UNIT: &str = "libs.kimath.src.geometry.eda_angle.application.pure.p001";

impl EdaAngle {
    /// `EDA_ANGLE::KeepUpright`: 0 degrees for angles within 45 degrees of horizontal (either
    /// way up, boundaries as in the source), otherwise 90 degrees.
    pub fn keep_upright(self) -> Self {
        let mut in_angle = self;
        in_angle.normalize();

        let in_degrees = in_angle.as_degrees();
        let out_degrees = if in_degrees <= 45.0
            || in_degrees >= 315.0
            || (in_degrees > 135.0 && in_degrees <= 225.0)
        {
            0.0
        } else {
            90.0
        };

        Self::new(out_degrees, AngleUnit::Degrees)
    }

    /// `EDA_ANGLE::IsCardinal`: whether the angle is an exact multiple of 90 degrees after
    /// folding into [0, 90).
    pub fn is_cardinal(self) -> bool {
        let value = self.as_degrees();
        if !value.is_finite() {
            return false;
        }

        let mut test = Self::fold_to_two_periods(value, 90.0);

        while test < 0.0 {
            test = fp::add(test, 90.0);
        }

        while test >= 90.0 {
            test = fp::sub(test, 90.0);
        }

        test == 0.0
    }

    /// `EDA_ANGLE::IsCardinal90`: whether the angle is one of the two cardinal directions
    /// 90 or 270 degrees, after folding its magnitude into [0, 180).
    pub fn is_cardinal90(self) -> bool {
        // return true if angle is one of the two cardinal directions (90/270 degrees),
        let value = self.as_degrees();
        if !value.is_finite() {
            return false;
        }

        let mut test = Self::fold_to_two_periods(value.abs(), 180.0);

        while test >= 180.0 {
            test = fp::sub(test, 180.0);
        }

        test == 90.0
    }
}

#[cfg(test)]
mod tests {
    use crate::eda_angle::EdaAngle;

    // f64::next_up is newer than the workspace minimum Rust; step positive values by bits.
    fn up(value: f64) -> f64 {
        f64::from_bits(value.to_bits() + 1)
    }
    fn down(value: f64) -> f64 {
        f64::from_bits(value.to_bits() - 1)
    }

    fn upright(degrees: f64) -> u64 {
        EdaAngle::degrees(degrees)
            .keep_upright()
            .as_degrees()
            .to_bits()
    }

    #[test]
    fn keep_upright_boundaries_follow_the_source_comparisons() {
        let zero = 0.0_f64.to_bits();
        let ninety = 90.0_f64.to_bits();
        assert_eq!(upright(45.0), zero);
        assert_eq!(upright(up(45.0)), ninety);
        assert_eq!(upright(135.0), ninety);
        assert_eq!(upright(up(135.0)), zero);
        assert_eq!(upright(225.0), zero);
        assert_eq!(upright(up(225.0)), ninety);
        assert_eq!(upright(down(315.0)), ninety);
        assert_eq!(upright(315.0), zero);
        assert_eq!(upright(-90.0), ninety);
        assert_eq!(upright(f64::NAN), zero);
        assert_eq!(upright(f64::NEG_INFINITY), zero);
    }

    #[test]
    fn cardinal_tests_fold_and_reject_non_finite_values() {
        let cardinal = |d: f64| EdaAngle::degrees(d).is_cardinal();
        let cardinal90 = |d: f64| EdaAngle::degrees(d).is_cardinal90();
        assert!(cardinal(0.0) && cardinal(-0.0) && cardinal(-270.0) && cardinal(90.0 * 1e18));
        assert!(!cardinal(1e-300) && !cardinal(f64::NAN) && !cardinal(f64::INFINITY));
        // -1e-300 + 90 rounds to 90, which the second loop folds to exactly zero.
        assert!(cardinal(-1e-300));
        assert!(cardinal90(90.0) && cardinal90(-90.0) && cardinal90(270.0) && cardinal90(-450.0));
        assert!(!cardinal90(0.0) && !cardinal90(180.0) && !cardinal90(f64::NEG_INFINITY));
    }
}
