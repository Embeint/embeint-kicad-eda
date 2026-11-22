// SPDX-License-Identifier: GPL-3.0-or-later
//! Literal interface scales from the final accounted units_scales.h span.

pub const UNIT_KM: f64 = 1e3;
pub const UNIT_M: f64 = 1.0;
pub const UNIT_CM: f64 = 1e-2;
pub const UNIT_MM: f64 = 1e-3;
pub const UNIT_MICRON: f64 = 1e-6;
pub const UNIT_INCH: f64 = 1e-2 * 2.54;
pub const UNIT_MIL: f64 = 1e-5 * 2.54;
pub const UNIT_FEET: f64 = 0.3048;
pub const UNIT_OZSQFT: f64 = 34.40 * UNIT_MICRON;
pub const UNIT_GHZ: f64 = 1e9;
pub const UNIT_MHZ: f64 = 1e6;
pub const UNIT_KHZ: f64 = 1e3;
pub const UNIT_DEGREE: f64 = std::f64::consts::PI / 180.0;
pub const UNIT_RADIAN: f64 = 1.0;
pub const UNIT_OHM: f64 = 1.0;
pub const UNIT_KOHM: f64 = 1e3;
pub const UNIT_OHM_PER_METER: f64 = 1.0;
pub const UNIT_OHM_PER_KILOMETER: f64 = 1e-3;
pub const UNIT_OHM_PER_FEET: f64 = 3.28084;
pub const UNIT_OHM_PER_1000FEET: f64 = 3.28084e-3;
pub const UNIT_KILOVOLT: f64 = 1e-3;
pub const UNIT_VOLT: f64 = 1.0;
pub const UNIT_MILLIVOLT: f64 = 1e+3;
pub const UNIT_MEGAWATT: f64 = 1e-6;
pub const UNIT_KILOWATT: f64 = 1e-3;
pub const UNIT_WATT: f64 = 1.0;
pub const UNIT_MILLIWATT: f64 = 1e+3;
pub const UNIT_METER_PER_SECOND: f64 = 1.0;
pub const UNIT_KILOMETER_PER_HOUR: f64 = 1.0 / 3.6;
pub const UNIT_FEET_PER_SECOND: f64 = 0.3048;
pub const UNIT_MILES_PER_HOUR: f64 = 1609.34;
pub const UNIT_SECOND: f64 = 1.0;
pub const UNIT_MSECOND: f64 = 1e-3;
pub const UNIT_USECOND: f64 = 1e-6;
pub const UNIT_NSECOND: f64 = 1e-9;
pub const UNIT_PSECOND: f64 = 1e-12;

pub const VALUES: &[(&str, f64)] = &[
    ("UNIT_KM", UNIT_KM),
    ("UNIT_M", UNIT_M),
    ("UNIT_CM", UNIT_CM),
    ("UNIT_MM", UNIT_MM),
    ("UNIT_MICRON", UNIT_MICRON),
    ("UNIT_INCH", UNIT_INCH),
    ("UNIT_MIL", UNIT_MIL),
    ("UNIT_FEET", UNIT_FEET),
    ("UNIT_OZSQFT", UNIT_OZSQFT),
    ("UNIT_GHZ", UNIT_GHZ),
    ("UNIT_MHZ", UNIT_MHZ),
    ("UNIT_KHZ", UNIT_KHZ),
    ("UNIT_DEGREE", UNIT_DEGREE),
    ("UNIT_RADIAN", UNIT_RADIAN),
    ("UNIT_OHM", UNIT_OHM),
    ("UNIT_KOHM", UNIT_KOHM),
    ("UNIT_OHM_PER_METER", UNIT_OHM_PER_METER),
    ("UNIT_OHM_PER_KILOMETER", UNIT_OHM_PER_KILOMETER),
    ("UNIT_OHM_PER_FEET", UNIT_OHM_PER_FEET),
    ("UNIT_OHM_PER_1000FEET", UNIT_OHM_PER_1000FEET),
    ("UNIT_KILOVOLT", UNIT_KILOVOLT),
    ("UNIT_VOLT", UNIT_VOLT),
    ("UNIT_MILLIVOLT", UNIT_MILLIVOLT),
    ("UNIT_MEGAWATT", UNIT_MEGAWATT),
    ("UNIT_KILOWATT", UNIT_KILOWATT),
    ("UNIT_WATT", UNIT_WATT),
    ("UNIT_MILLIWATT", UNIT_MILLIWATT),
    ("UNIT_METER_PER_SECOND", UNIT_METER_PER_SECOND),
    ("UNIT_KILOMETER_PER_HOUR", UNIT_KILOMETER_PER_HOUR),
    ("UNIT_FEET_PER_SECOND", UNIT_FEET_PER_SECOND),
    ("UNIT_MILES_PER_HOUR", UNIT_MILES_PER_HOUR),
    ("UNIT_SECOND", UNIT_SECOND),
    ("UNIT_MSECOND", UNIT_MSECOND),
    ("UNIT_USECOND", UNIT_USECOND),
    ("UNIT_NSECOND", UNIT_NSECOND),
    ("UNIT_PSECOND", UNIT_PSECOND),
];
