// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests of the glibc 2.39 `sin`/`cos`/`sincos` port.
//!
//! In-tree data (all compared bit for bit, on every target including wasm):
//! - `genuine_*`: retained outputs of the locked glibc in the reference image (runtime data).
//! - `emulated_domain.txt`: static emulation of the locked machine code (not a runtime oracle).
//!
//! The ignored tests `reference_write_domain` and `reference_records` compare the full
//! fixed-seed domain with records produced by the genuine library in hosted CI; see
//! `rust/kicad-math/tests/data/glibc_trig/README.md`.

use super::{cos, sin, sincos};
use crate::matrix3x3::Matrix3x3;

const LIBM_VALUES: &str = include_str!("../../tests/data/glibc_trig/genuine_libm_values.txt");
const SET_ROTATION: &str = include_str!("../../tests/data/glibc_trig/genuine_set_rotation.txt");
const SWEEP_SAMPLE: &str = include_str!("../../tests/data/glibc_trig/genuine_sweep_sample.txt");
const EMULATED: &str = include_str!("../../tests/data/glibc_trig/emulated_domain.txt");

fn hex(text: &str) -> u64 {
    let value = u64::from_str_radix(text, 16);
    assert!(value.is_ok(), "bad hex field {text:?}");
    value.unwrap_or(0)
}

fn rows(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split_whitespace().collect())
}

fn bits(x: f64) -> u64 {
    x.to_bits()
}

/// Checks all three functions for one input against expected bits; returns mismatch text.
fn check(x: u64, want: [u64; 4]) -> Option<String> {
    let value = f64::from_bits(x);
    let (s, c) = sincos(value);
    let got = [bits(sin(value)), bits(cos(value)), bits(s), bits(c)];
    (got != want).then(|| format!("x {x:016x}: got {got:016x?}, want {want:016x?}"))
}

#[test]
#[allow(clippy::excessive_precision)]
fn source_literals() {
    // s_sin.c decimal literals round to the bit patterns used by the port.
    assert_eq!(super::SN3, -1.66666666666664880952546298448555E-01);
    assert_eq!(super::SN5, 8.33333214285722277379541354343671E-03);
    assert_eq!(super::CS2, 4.99999999999999999999950396842453E-01);
    assert_eq!(super::CS4, -4.16666666666664434524222570944589E-02);
    assert_eq!(super::CS6, 1.38888874007937613028114285595617E-03);
    assert_eq!(super::BR_SPLIT, 134217729.0);
    // usncs.h: big and toint are 1.5 * 2^45 and 1.5 * 2^52.
    assert_eq!(super::BIG, 1.5 * 35_184_372_088_832.0);
    assert_eq!(super::TOINT, 6_755_399_441_055_744.0);
    // Every toverp digit is an integer below 2^24.
    assert!(super::TOVERP
        .iter()
        .all(|d| *d >= 0.0 && *d < 16_777_216.0 && *d == (*d as u32) as f64));
}

#[test]
fn genuine_libm_values() {
    let mut count = 0;
    for f in rows(LIBM_VALUES) {
        assert_eq!(f.len(), 5);
        let want = [hex(f[1]), hex(f[2]), hex(f[3]), hex(f[4])];
        assert_eq!(check(hex(f[0]), want), None);
        count += 1;
    }
    assert_eq!(count, 37);
}

#[test]
fn genuine_set_rotation() {
    let (mut doubles, mut ints, mut sensitive) = (0, 0, 0);
    for f in rows(SET_ROTATION) {
        assert_eq!(f.len(), 5, "{f:?}");
        if let Some(angle) = f[1].strip_prefix('i') {
            let parsed = angle.parse::<i32>();
            assert!(parsed.is_ok(), "{f:?}");
            let mut m = Matrix3x3::<i32>::new();
            m.set_rotation(parsed.unwrap_or(0));
            let (want_sin, want_cos) = (f[2].parse::<i32>(), f[3].parse::<i32>());
            assert!(want_sin.is_ok() && want_cos.is_ok(), "{f:?}");
            let (want_sin, want_cos) = (want_sin.unwrap_or(9), want_cos.unwrap_or(9));
            assert_eq!(
                [m.data[0][0], m.data[0][1], m.data[1][0], m.data[1][1]],
                [want_cos, -want_sin, want_sin, want_cos],
                "{}",
                f[0]
            );
            ints += 1;
            continue;
        }
        let angle = f64::from_bits(hex(f[1]));
        let (want_sin, want_cos) = (hex(f[2]), hex(f[3]));
        let (s, c) = sincos(angle);
        assert_eq!((bits(s), bits(c)), (want_sin, want_cos), "{}", f[0]);
        let mut m = Matrix3x3::<f64>::new();
        m.set_rotation(angle);
        assert_eq!(
            [m.data[0][0], m.data[0][1], m.data[1][0], m.data[1][1]].map(bits),
            [want_cos, want_sin ^ (1 << 63), want_sin, want_cos],
            "{}",
            f[0]
        );
        doubles += 1;
        sensitive += usize::from(f[4] == "1");
    }
    assert_eq!((doubles, ints, sensitive), (285, 40, 30));
}

#[test]
fn genuine_sweep_sample() {
    let mut count = 0;
    for f in rows(SWEEP_SAMPLE) {
        assert_eq!(f.len(), 4);
        let x = f64::from_bits(hex(f[1]));
        assert_eq!(
            (bits(sin(x)), bits(cos(x))),
            (hex(f[2]), hex(f[3])),
            "sweep index {}",
            f[0]
        );
        count += 1;
    }
    assert!(count >= 3000, "{count}");
}

#[test]
fn emulated_domain() {
    let mut failures = Vec::new();
    let mut count = 0;
    for f in rows(EMULATED) {
        assert_eq!(f.len(), 5);
        let want = [hex(f[1]), hex(f[2]), hex(f[3]), hex(f[4])];
        failures.extend(check(hex(f[0]), want));
        count += 1;
    }
    assert_eq!(count, domain::fixed_inputs().len() + 1000);
    assert!(
        failures.is_empty(),
        "{} mismatches: {:?}",
        failures.len(),
        &failures[..failures.len().min(10)]
    );
}

#[test]
fn signed_zero_and_specials() {
    assert_eq!(bits(sin(-0.0)), 0x8000_0000_0000_0000);
    assert_eq!(bits(cos(-0.0)), 0x3ff0_0000_0000_0000);
    assert_eq!(sincos(-0.0).0.to_bits(), 0x8000_0000_0000_0000);
    for (x, want) in [
        (0x7ff0_0000_0000_0000_u64, 0xfff8_0000_0000_0000_u64),
        (0xfff0_0000_0000_0000, 0xfff8_0000_0000_0000),
        (0x7ff0_0000_0000_0001, 0x7ff8_0000_0000_0001), // sNaN: quietened, payload kept
        (0xfff4_0000_0000_0abc, 0xfffc_0000_0000_0abc),
        (0x7ff8_0000_dead_beef, 0x7ff8_0000_dead_beef),
        (0xffff_ffff_ffff_ffff, 0xffff_ffff_ffff_ffff),
    ] {
        assert_eq!(check(x, [want; 4]), None);
    }
}

#[test]
fn domain_generator_matches_python() {
    // Values printed by domain.py (FNV-1a 64 over little-endian bytes).
    let fixed = domain::fixed_inputs();
    assert_eq!(fixed.len(), 1602);
    assert_eq!(domain::fnv1a64(&fixed), 0x8e0d_3211_5a0e_d300);
    let seeded = domain::generate(4096);
    assert_eq!(
        &seeded[..4],
        &[
            0xda0c_8f77_8971_b951,
            0x5903_a160_6521_a3a6,
            0xf40f_910d_a53d_54e3,
            0x425f_ab03_2588_a464
        ]
    );
    assert_eq!(domain::fnv1a64(&seeded), 0x16a6_6594_523e_21cc);
}

/// Writes the reference domain (fixed inputs, then `KICAD_GLIBC_TRIG_COUNT` seeded inputs,
/// default 1,000,000) as little-endian u64 to `KICAD_GLIBC_TRIG_DOMAIN`.
#[test]
#[ignore = "hosted CI: produces the input file for the genuine reference probe"]
fn reference_write_domain() {
    let path = std::env::var("KICAD_GLIBC_TRIG_DOMAIN");
    assert!(path.is_ok(), "set KICAD_GLIBC_TRIG_DOMAIN");
    let mut out = Vec::new();
    for x in domain::reference_inputs() {
        out.extend_from_slice(&x.to_le_bytes());
    }
    let written = std::fs::write(path.unwrap_or_default(), out);
    assert!(written.is_ok(), "{written:?}");
}

/// Compares every record of `KICAD_GLIBC_TRIG_REFERENCE` (little-endian u64 quintuples
/// `x, sin, cos, sincos_sin, sincos_cos` from the genuine library) with the port, and checks
/// that the inputs are exactly the reference domain.
#[test]
#[ignore = "hosted CI: needs the genuine reference records"]
fn reference_records() {
    let path = std::env::var("KICAD_GLIBC_TRIG_REFERENCE");
    assert!(path.is_ok(), "set KICAD_GLIBC_TRIG_REFERENCE");
    let data = std::fs::read(path.unwrap_or_default());
    assert!(data.is_ok(), "{data:?}");
    let data = data.unwrap_or_default();
    assert_eq!(data.len() % 40, 0);
    let inputs = domain::reference_inputs();
    assert_eq!(data.len() / 40, inputs.len(), "record count");
    let mut failures = Vec::new();
    for (record, x) in data.chunks_exact(40).zip(inputs) {
        let mut words = [0u64; 5];
        for (word, chunk) in words.iter_mut().zip(record.chunks_exact(8)) {
            let mut le = [0u8; 8];
            le.copy_from_slice(chunk);
            *word = u64::from_le_bytes(le);
        }
        assert_eq!(words[0], x, "reference input order");
        failures.extend(check(x, [words[1], words[2], words[3], words[4]]));
    }
    println!(
        "glibc_trig reference: {} records, {} mismatches",
        data.len() / 40,
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{:?}",
        &failures[..failures.len().min(20)]
    );
}

/// Fixed-seed binary64 domain; identical to `domain.py` and checked against it above.
mod domain {
    use std::collections::BTreeSet;
    use std::f64::consts::PI;

    pub(super) const SEED: u64 = 0x5369_6e43_6f73_476c; // "SinCosGl"

    struct SplitMix(u64);

    impl SplitMix {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
    }

    /// Moves a finite double by `delta` ULPs along the ordered line, clamped to +-DBL_MAX.
    fn step_ulps(bits: u64, delta: i64) -> u64 {
        let ordered = if bits >> 63 != 0 {
            -((bits & 0x7fff_ffff_ffff_ffff) as i64)
        } else {
            bits as i64
        };
        let limit = 0x7fef_ffff_ffff_ffff_i64;
        let ordered = (ordered + delta).clamp(-limit, limit);
        if ordered < 0 {
            0x8000_0000_0000_0000 | (-ordered) as u64
        } else {
            ordered as u64
        }
    }

    fn bounds() -> Vec<u64> {
        let mut out = vec![
            0x3e40_0000,
            0x3e50_0000,
            0x3feb_6000,
            0x4003_68fd,
            0x4199_21fb,
            0x7ff0_0000,
        ];
        for k in 0..48u64 {
            let original = 450 + 24 * k + 600;
            if (1..=2046).contains(&original) {
                out.push(original << 20);
            }
        }
        out
    }

    fn angle(rng: &mut SplitMix, index: u64, bounds: &[u64]) -> u64 {
        let r = rng.next();
        let sign = (r & 1) << 63;
        match index % 8 {
            0 => r,
            1 => {
                let exponent = (1023 - 60 + (r >> 52) % (60 + 1024)).min(2046);
                ((r >> 63) << 63) | (exponent << 52) | (r & 0x000f_ffff_ffff_ffff)
            }
            2 => {
                let exponent = 1023 + 26 + (r >> 52) % (2047 - 1023 - 26);
                ((r >> 63) << 63) | (exponent << 52) | (r & 0x000f_ffff_ffff_ffff)
            }
            3 => {
                let k = (r >> 24) & ((1 << 40) - 1);
                let base = (k as f64 * (PI / 2.0)).to_bits();
                step_ulps(base, ((r >> 8) & 31) as i64 - 16) | sign
            }
            4 => {
                let base = bounds[((r >> 32) % bounds.len() as u64) as usize] << 32;
                let stepped = if base < 0x7ff0_0000_0000_0000 {
                    step_ulps(base, ((r >> 8) & 127) as i64 - 64)
                } else {
                    step_ulps(0x7fef_ffff_ffff_ffff, -(((r >> 8) & 63) as i64))
                };
                stepped | sign
            }
            5 => {
                let base = if (r >> 60) & 1 != 0 {
                    0.126f64.to_bits()
                } else {
                    let j = (r >> 32) % 110;
                    ((j as f64 + 0.5) / 128.0).to_bits()
                };
                step_ulps(base, ((r >> 8) & 15) as i64 - 8) | sign
            }
            6 => {
                let u = (r >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0);
                ((u * 2.0 - 1.0) * 2.5).to_bits()
            }
            _ => {
                if (r >> 62) & 1 != 0 {
                    let n = (r >> 1) % 3_600_001;
                    (n as f64 / 10.0 * (PI / 180.0)).to_bits() | sign
                } else {
                    let magnitude = if (r >> 61) & 1 != 0 {
                        (r >> 2) % (1 << 53)
                    } else {
                        (r >> 2) % 100_000
                    };
                    (magnitude as f64).to_bits() | sign
                }
            }
        }
    }

    pub(super) fn generate(count: u64) -> Vec<u64> {
        let bounds = bounds();
        let mut rng = SplitMix(SEED);
        (0..count).map(|i| angle(&mut rng, i, &bounds)).collect()
    }

    fn pow2(exponent: i32) -> f64 {
        f64::from_bits(((1023 + exponent) as u64) << 52)
    }

    pub(super) fn fixed_inputs() -> Vec<u64> {
        let mut out = BTreeSet::new();
        for bound in bounds() {
            let base = bound << 32;
            for delta in -2..=2i64 {
                let stepped = if base == 0x7ff0_0000_0000_0000 {
                    step_ulps(0x7fef_ffff_ffff_ffff, -(0.max(-delta)))
                } else {
                    step_ulps(base, delta)
                };
                out.insert(stepped);
                out.insert(stepped | (1 << 63));
            }
            out.insert(base | 0xffff_ffff);
            out.insert(((bound - 1) << 32) | 0xffff_ffff);
        }
        let specials = [
            0x0000_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x000f_ffff_ffff_ffff,
            0x0010_0000_0000_0000,
            0x3ff0_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
            0x7ff0_0000_0000_0000,
            0x7ff0_0000_0000_0001,
            0x7ff4_0000_0000_0abc,
            0x7ff7_ffff_ffff_ffff,
            0x7ff8_0000_0000_0000,
            0x7ff8_0000_0000_0123,
            0x7fff_ffff_ffff_ffff,
            0x3fc0_20c4_9ba5_e354,
            0x3fc0_20c4_9ba5_e353,
            0x3fc0_20c4_9ba5_e355,
            (PI / 4.0).to_bits(),
            (PI / 2.0).to_bits(),
            PI.to_bits(),
            (2.0 * PI).to_bits(),
            105_414_350.0f64.to_bits(),
            0x5fc2_b9ae_a05c_c4d6,
            (6_381_956_970_095_103.0 * pow2(797)).to_bits(),
            5.319_372_648_326_541e255f64.to_bits(),
            1e22f64.to_bits(),
            1e300f64.to_bits(),
            pow2(1023).to_bits(),
            1.0e-300f64.to_bits(),
            pow2(-27).to_bits(),
            pow2(-26).to_bits(),
        ];
        for s in specials {
            out.insert(s);
            out.insert(s | (1 << 63));
        }
        for j in 0..110 {
            let base = ((j as f64 + 0.5) / 128.0).to_bits();
            for delta in -1..=1 {
                out.insert(step_ulps(base, delta));
                out.insert(step_ulps(base, delta) | (1 << 63));
            }
        }
        for k in 1..64 {
            let base = (k as f64 * (PI / 2.0)).to_bits();
            for delta in -1..=1 {
                out.insert(step_ulps(base, delta));
                out.insert(step_ulps(base, delta) | (1 << 63));
            }
        }
        out.into_iter().collect()
    }

    /// The hosted-CI reference domain: fixed inputs, then `KICAD_GLIBC_TRIG_COUNT` seeded.
    pub(super) fn reference_inputs() -> Vec<u64> {
        let count = std::env::var("KICAD_GLIBC_TRIG_COUNT")
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
            .unwrap_or(1_000_000);
        let mut out = fixed_inputs();
        out.extend(generate(count));
        out
    }

    pub(super) fn fnv1a64(values: &[u64]) -> u64 {
        let mut state = 0xcbf2_9ce4_8422_2325_u64;
        for v in values {
            for byte in v.to_le_bytes() {
                state = (state ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        state
    }
}
