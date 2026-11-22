// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_iec60664::{Field, Iec60664, InsulationType, MaterialGroup, PollutionDegree};

#[test]
fn setters_do_not_recompute_and_instances_do_not_share_state() {
    let mut calc: Iec60664 = kicad_iec60664::iec60664::Iec60664::new();
    calc.set_altitude(2000.0);
    calc.set_peak_voltage(0.4);
    calc.set_transient_voltage(0.33);
    calc.set_rms_voltage(230.0);
    calc.set_material_group(MaterialGroup::I);
    calc.set_pollution_degree(PollutionDegree::PD2);
    calc.set_insulation_type(InsulationType::BASIC);
    assert!(calc.compute());
    assert!(calc.clearance_distance() > 0.0);
    let calculated = calc.state();
    calc.set_altitude(9000.0);
    assert_eq!(
        calc.clearance_distance().to_bits(),
        calculated.clearance.to_bits()
    );
    assert!(calc.compute());
    assert!(calc.clearance_distance() > calculated.clearance);
    assert_eq!(Iec60664::new().clearance_distance(), -1.0);
}

#[test]
fn finite_extremes_and_unnamed_categories_are_total() {
    let mut calc = Iec60664::new();
    for n in [
        f64::MIN,
        -f64::MIN_POSITIVE,
        -f64::from_bits(1),
        -0.0,
        0.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::MAX,
    ] {
        calc.set_altitude(n);
        calc.set_frequency(n);
        calc.set_rated_voltage(n);
        calc.set_rms_voltage(n);
        calc.set_peak_voltage(n);
        calc.set_transient_voltage(n);
        for category in [i32::MIN, 0, i32::MAX] {
            calc.set_field(Field(category));
            calc.set_material_group(MaterialGroup(category));
            calc.set_pollution_degree(PollutionDegree(category));
            assert!(calc.compute());
            assert!(calc.clearance_distance().is_finite());
            assert!(calc.creepage_distance().is_finite());
            assert!(calc.min_groove_width().is_finite());
        }
    }
}
