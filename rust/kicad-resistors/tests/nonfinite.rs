// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_resistors::{testing, Resistance};

#[test]
fn distinct_nan_operands_keep_reference_second_sign_and_payload() {
    for first in [0x7ff8_0000_0000_0001, 0xfff0_0000_0000_0002] {
        for second in [0x7ff8_0000_0000_0003, 0xfff0_0000_0000_0004] {
            let a = Resistance::new(f64::from_bits(first), "a".into(), vec![1.]);
            let b = Resistance::new(f64::from_bits(second), "b".into(), vec![1.]);
            for parallel in [false, true] {
                for simple in [false, true] {
                    let result = testing::combine(&a, &b, parallel, simple);
                    assert_eq!(result.value.to_bits(), second | 0x0008_0000_0000_0000);
                    assert_eq!(a.value.to_bits(), first);
                    assert_eq!(b.value.to_bits(), second);
                }
            }
        }
    }
}

#[test]
fn invalid_operations_without_nan_inputs_generate_reference_indefinite() {
    for (a, b, parallel) in [
        (f64::INFINITY, f64::NEG_INFINITY, false),
        (0., 0., true),
        (f64::INFINITY, 1., true),
        (f64::MAX, f64::MAX, true),
    ] {
        let a = Resistance::new(a, "a".into(), vec![1.]);
        let b = Resistance::new(b, "b".into(), vec![1.]);
        for simple in [false, true] {
            assert_eq!(
                testing::combine(&a, &b, parallel, simple).value.to_bits(),
                0xfff8_0000_0000_0000
            );
        }
    }
}
