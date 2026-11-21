// SPDX-License-Identifier: GPL-3.0-or-later
//! Code parity with KiCad's IEC60664-1:2020-05 calculator at the port pin.
//! This preserves its tables and sentinels; it is not electrical safety certification.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod tables;

// Fixed-underlying-type C++ enums permit unnamed integer values. Keep those values
// so the reference's default switch arms and ordered pollution tests remain reachable.
macro_rules! category {
    ($name:ident { $($constant:ident = $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(pub i32);
        impl $name { $(pub const $constant: Self = Self($value);)+ }
    };
}
category!(PollutionDegree { PD1 = 1, PD2 = 2, PD3 = 3, PD4 = 4 });
category!(OvervoltageCategory { I = 1, II = 2, III = 3, IV = 4 });
category!(MaterialGroup { I = 1, II = 2, IIIA = 3, IIIB = 4, NOT_INSULATING = 5 });
category!(InsulationType { FUNCTIONAL = 1, BASIC = 2, REINFORCED = 3 });
category!(Field { INHOMOGENEOUS = 1, HOMOGENEOUS = 2 });
category!(Waveform { AC_50_OR_60HZ = 1, IMPULSE_1_2_50 = 2 });

/// Value snapshot also exposes otherwise dormant input state to parity audits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub rated_voltage: f64,
    pub frequency: f64,
    pub coated: bool,
    pub pcb_material: bool,
    pub rms_voltage: f64,
    pub peak_voltage: f64,
    pub transient_voltage: f64,
    pub altitude: f64,
    pub clearance: f64,
    pub creepage: f64,
    pub min_groove_width: f64,
    pub field: Field,
    pub overvoltage_category: OvervoltageCategory,
    pub material_group: MaterialGroup,
    pub pollution_degree: PollutionDegree,
    pub insulation_type: InsulationType,
}

#[derive(Clone, Debug)]
pub struct Iec60664 {
    state: State,
}

impl Default for Iec60664 {
    fn default() -> Self {
        Self {
            state: State {
                rated_voltage: 100e3,
                frequency: 50.0,
                coated: false,
                pcb_material: false,
                rms_voltage: 100e3,
                peak_voltage: 100e3,
                transient_voltage: 100e3,
                altitude: 100e3,
                clearance: -1.0,
                creepage: -1.0,
                min_groove_width: -1.0,
                field: Field::INHOMOGENEOUS,
                overvoltage_category: OvervoltageCategory::IV,
                material_group: MaterialGroup::NOT_INSULATING,
                pollution_degree: PollutionDegree::PD3,
                insulation_type: InsulationType::REINFORCED,
            },
        }
    }
}

impl Iec60664 {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn set_altitude(&mut self, value: f64) {
        self.state.altitude = value;
    }
    pub fn set_pollution_degree(&mut self, value: PollutionDegree) {
        self.state.pollution_degree = value;
    }
    pub fn set_pcb_material(&mut self, value: bool) {
        self.state.pcb_material = value;
    }
    pub fn set_material_group(&mut self, value: MaterialGroup) {
        self.state.material_group = value;
    }
    pub fn set_frequency(&mut self, value: f64) {
        self.state.frequency = value;
    }
    pub fn set_insulation_type(&mut self, value: InsulationType) {
        self.state.insulation_type = value;
    }
    pub fn set_overvoltage_category(&mut self, value: OvervoltageCategory) {
        self.state.overvoltage_category = value;
    }
    pub fn set_field(&mut self, value: Field) {
        self.state.field = value;
    }
    pub fn set_rated_voltage(&mut self, value: f64) {
        self.state.rated_voltage = value;
    }
    pub fn set_rms_voltage(&mut self, value: f64) {
        self.state.rms_voltage = value;
    }
    pub fn set_peak_voltage(&mut self, value: f64) {
        self.state.peak_voltage = value;
    }
    pub fn set_transient_voltage(&mut self, value: f64) {
        self.state.transient_voltage = value;
    }
    pub fn min_groove_width(&self) -> f64 {
        self.state.min_groove_width
    }
    pub fn clearance_distance(&self) -> f64 {
        self.state.clearance
    }
    pub fn creepage_distance(&self) -> f64 {
        self.state.creepage
    }

    pub fn rated_impulse_withstand_voltage(&self) -> f64 {
        tables::impulse(self.state.rated_voltage, self.state.overvoltage_category)
    }

    /// Upstream's private overload defaults aDistIso to 100.
    pub fn groove_width(&self, pd: PollutionDegree, distance: Option<f64>) -> f64 {
        let distance = distance.unwrap_or(100.0);
        if distance <= 0.0 {
            return -1.0;
        }
        if distance.abs() < 3.0 {
            return distance / 3.0;
        }
        match pd {
            PollutionDegree::PD1 => 0.25,
            PollutionDegree::PD2 => 1.0,
            PollutionDegree::PD3 => 1.5,
            _ => -1.0,
        }
    }

    pub fn altitude_correction_factor(&self, altitude: f64) -> f64 {
        tables::altitude(altitude)
    }

    /// This helper has no active caller in the pinned application.
    /// Its C++ default field is INHOMOGENEOUS.
    pub fn clearance_for_rms_voltage(&self, voltage: f64, field: Option<Field>) -> f64 {
        tables::rms(voltage, field.unwrap_or(Field::INHOMOGENEOUS))
    }

    /// This helper has no active caller in the pinned application.
    pub fn material_group_from_cti(&self, cti: f64) -> MaterialGroup {
        if cti >= 600.0 {
            return MaterialGroup::I;
        }
        if cti >= 400.0 {
            return MaterialGroup::II;
        }
        if cti >= 175.0 {
            return MaterialGroup::IIIA;
        }
        if cti >= 100.0 {
            return MaterialGroup::IIIB;
        }
        MaterialGroup::NOT_INSULATING
    }

    pub fn clearance_to_withstand_transient_voltage(
        &self,
        voltage: f64,
        pd: PollutionDegree,
        field: Field,
    ) -> f64 {
        tables::transient(voltage, pd, field)
    }

    pub fn clearance_to_withstand_peaks(&self, voltage: f64, field: Field) -> f64 {
        tables::peaks(voltage, field)
    }

    pub fn basic_creepage_distance(
        &self,
        voltage: f64,
        pd: PollutionDegree,
        mg: MaterialGroup,
    ) -> f64 {
        tables::creepage(voltage, pd, mg, self.state.pcb_material)
    }

    pub fn compute_clearance_distance(
        &self,
        pd: PollutionDegree,
        field: Field,
        altitude: f64,
    ) -> f64 {
        // Frequency/coating refusals in the source are inside #if 0. They must
        // remain inactive, including above 30 kHz; neither setter changes this.
        let mut transient_voltage = self.state.transient_voltage;
        let reinforced = self.state.insulation_type == InsulationType::REINFORCED;
        if reinforced {
            transient_voltage = if transient_voltage == 0.33 {
                0.5
            } else if transient_voltage == 0.5 {
                0.8
            } else if transient_voltage == 0.8 {
                1.5
            } else if transient_voltage == 1.5 {
                2.5
            } else if transient_voltage == 2.5 {
                4.0
            } else if transient_voltage == 4.0 {
                6.0
            } else if transient_voltage == 6.0 {
                8.0
            } else if transient_voltage == 8.0 {
                12.0
            } else {
                transient_voltage * 1.6
            };
        }
        let mut clearance1 =
            self.clearance_to_withstand_transient_voltage(transient_voltage, pd, field);
        let mut clearance2 = self.clearance_to_withstand_peaks(
            if reinforced {
                self.state.peak_voltage * 1.6
            } else {
                self.state.peak_voltage
            },
            field,
        );
        if clearance1 == -1.0 || clearance2 == -1.0 {
            clearance1 = -1.0;
            clearance2 = -1.0;
        }
        let mut clearance = if clearance1 > clearance2 {
            clearance1
        } else {
            clearance2
        };
        clearance *= self.altitude_correction_factor(altitude);
        clearance
    }

    pub fn compute_creepage_distance(&self, pd: PollutionDegree, mg: MaterialGroup) -> f64 {
        let mut creepage = self.basic_creepage_distance(self.state.rms_voltage, pd, mg);
        if self.state.insulation_type == InsulationType::REINFORCED {
            creepage *= 2.0;
        }
        creepage
    }

    pub fn compute(&mut self) -> bool {
        self.state.clearance = self.compute_clearance_distance(
            self.state.pollution_degree,
            self.state.field,
            self.state.altitude,
        );
        self.state.creepage =
            self.compute_creepage_distance(self.state.pollution_degree, self.state.material_group);
        if self.state.creepage < self.state.clearance || self.state.clearance <= 0.0 {
            self.state.creepage = self.state.clearance;
        }
        self.state.min_groove_width =
            self.groove_width(self.state.pollution_degree, Some(self.state.clearance));
        true
    }
}
